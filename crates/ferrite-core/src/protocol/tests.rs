//! The generation-1 contract, frozen: every message and every `live` input
//! kind has a fixture under `fixtures/gen1`, and each decodes to what this
//! build means by it and re-encodes byte for byte. Fixtures are append-only:
//! never edit one; a new optional field gets a new fixture beside the old.
//! A missing fixture is written by running with `FERRITE_BLESS_FIXTURES=1`,
//! which never overwrites one.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::activity::{ActivityInput, AgentKey, ExecutionEvent};

fn fixtures(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/protocol/fixtures")
        .join(kind)
}

enum Sample {
    Client(ClientMessage),
    Server(ServerMessage),
}

impl Sample {
    fn encode(&self) -> Vec<u8> {
        match self {
            Sample::Client(message) => encode(message),
            Sample::Server(message) => encode(message),
        }
    }

    /// Decode `line` as this sample's direction and compare.
    fn decodes_from(&self, line: &[u8]) -> bool {
        let line = line.strip_suffix(b"\n").expect("a fixture is one line");
        match self {
            Sample::Client(message) => decode::<ClientMessage>(line).unwrap() == *message,
            Sample::Server(message) => decode::<ServerMessage>(line).unwrap() == *message,
        }
    }
}

fn handle() -> WireHandle {
    WireHandle {
        generation: 3,
        serial: 1,
        request_id: "perm_01".into(),
    }
}

fn decision() -> WireDecision {
    WireDecision {
        id: "perm_01".into(),
        delivery: WireDelivery::Blocking,
        kind: WireDecisionKind::Approval,
        policy: WirePolicy {
            allow: true,
            deny: true,
            prefer_deny: false,
            interaction_required: false,
        },
        tool_use_id: "toolu_01".into(),
        tool_name: "Write".into(),
        description: "ferrite-perm.txt".into(),
        input: json!({"file_path": "ferrite-perm.txt"}),
        suggestions: vec![WireDecisionChoice {
            label: "always in this folder".into(),
            value: json!({"rule": "Write(./*)"}),
            standing: true,
        }],
    }
}

fn question_decision() -> WireDecision {
    WireDecision {
        kind: WireDecisionKind::Questions {
            questions: vec![WireQuestion {
                id: Some("q1".into()),
                question: "Which approach?".into(),
                header: "Approach".into(),
                options: vec![
                    WireQuestionChoice {
                        label: "A".into(),
                        description: "the first".into(),
                        preview: Some("fn a() {}".into()),
                    },
                    WireQuestionChoice {
                        label: "B".into(),
                        description: String::new(),
                        preview: None,
                    },
                ],
                multi_select: false,
                secret: false,
                allow_other: true,
            }],
        },
        tool_name: "AskUserQuestion".into(),
        suggestions: vec![],
        ..decision()
    }
}

fn form_decision() -> WireDecision {
    WireDecision {
        delivery: WireDelivery::Async,
        kind: WireDecisionKind::Form {
            fields: vec![
                WireFormField {
                    id: "name".into(),
                    label: "Name".into(),
                    description: String::new(),
                    required: true,
                    kind: WireFieldKind::String {
                        min_length: Some(1),
                        max_length: None,
                        default: None,
                    },
                },
                WireFormField {
                    id: "size".into(),
                    label: "Size".into(),
                    description: "how many".into(),
                    required: false,
                    kind: WireFieldKind::Enum {
                        options: vec![WireFormChoice {
                            value: "s".into(),
                            label: "Small".into(),
                        }],
                        multi_select: false,
                        min_items: None,
                        max_items: None,
                        default: Some(json!("s")),
                    },
                },
            ],
        },
        suggestions: vec![],
        ..decision()
    }
}

/// Literal, so a fixture never follows the display-name grooming.
fn model(value: &str) -> crate::ModelInfo {
    crate::ModelInfo {
        value: value.into(),
        display: value.to_uppercase(),
        detail: String::new(),
        resolved: None,
        efforts: vec!["low".into(), "high".into()],
        default_effort: Some("high".into()),
    }
}

/// Minted only by the registry; a fixture names one by its number.
fn project_id() -> ProjectId {
    serde_json::from_value(json!(2)).unwrap()
}

/// A parked Thread as a machine's Server lists it.
fn parked_row() -> ParkedThread {
    ParkedThread {
        thread: ThreadId::new(2),
        provider: Some(Provider::Codex),
        title: None,
        prompt_title: Some("Tidy the release notes".into()),
        workspace: Some(WireBinding::Main {
            checkout: "/src/ferrite".into(),
        }),
        session_project_root: None,
        project: Some(project_id()),
        last_used_ms: Some(1_759_990_000_000),
        turns: Some(11),
        subagents: Some(2),
    }
}

fn record(input: Input) -> WireRecord {
    WireRecord::Known(Box::new(
        LogRecord::of(&input, None).expect("the log keeps it"),
    ))
}

fn state() -> ThreadState {
    ThreadState {
        thread: ThreadId::new(4),
        generation: 3,
        provider: Provider::Claude,
        model: Some("sonnet".into()),
        effort: Some("high".into()),
        title: Some("Fix the parser".into()),
        workspace: Some(WireBinding::Worktree {
            repo: "/src/ferrite".into(),
            path: "/src/ferrite-wt/4".into(),
        }),
        session_project_root: None,
        first_prompt_sent: true,
        has_prompt_history: true,
        starting: false,
        working: true,
        tuning: false,
        native_queue_pending: false,
        queued: vec!["and then the tests".into()],
        suggestion: None,
        commands: vec![WireCommand {
            name: "review".into(),
            description: "Review the diff".into(),
            path: None,
        }],
        models: vec![model("sonnet")],
        permission_mode: Some("default".into()),
        permission_modes: vec![WirePermissionMode {
            value: "default".into(),
            label: "Default".into(),
        }],
        controls: vec![WireControlKind::RefreshContext, WireControlKind::StopTask],
        store_error: None,
        for_draft: false,
        recovered: vec![],
        wants_listing: false,
        history_errors: vec![],
    }
}

fn input(seq: u64, input: WireInput) -> Sample {
    Sample::Server(ServerMessage::Input {
        thread: ThreadId::new(4),
        seq,
        at_ms: 1_250 + seq,
        input,
    })
}

fn live(seq: u64, live: Live) -> Sample {
    input(seq, WireInput::Live(live))
}

fn command(id: u64, op: Op) -> Sample {
    Sample::Client(ClientMessage::Command(Command { id, op }))
}

fn samples() -> Vec<(&'static str, Sample)> {
    let thread = ThreadId::new(4);
    let group = GroupId::new(1);
    let project = project_id();
    let checkout = || WireChoice::Main {
        checkout: "/src/ferrite".into(),
    };
    vec![
        (
            "hello",
            Sample::Client(ClientMessage::Hello {
                generation: 1,
                client: Peer {
                    name: "ferrite".into(),
                    version: "0.9.0".into(),
                },
                capabilities: vec![Capability::RenderVisual],
            }),
        ),
        (
            "hello_parked_threads",
            Sample::Client(ClientMessage::Hello {
                generation: 1,
                client: Peer {
                    name: "ferrite".into(),
                    version: "0.9.0".into(),
                },
                capabilities: vec![Capability::ParkedThreads],
            }),
        ),
        (
            "watch",
            Sample::Client(ClientMessage::Watch {
                thread,
                resume: None,
            }),
        ),
        (
            "watch_resume",
            Sample::Client(ClientMessage::Watch {
                thread,
                resume: Some(Resume {
                    boot_id: "b-1".into(),
                    after: 41,
                }),
            }),
        ),
        ("unwatch", Sample::Client(ClientMessage::Unwatch { thread })),
        (
            "rendered",
            Sample::Client(ClientMessage::Rendered {
                request: 9,
                png: Some(WirePng {
                    base64: "iVBORw0KGgo=".into(),
                    width: 640,
                    height: 480,
                }),
                error: None,
            }),
        ),
        (
            "rendered_error",
            Sample::Client(ClientMessage::Rendered {
                request: 9,
                png: None,
                error: Some("no window is open".into()),
            }),
        ),
        (
            "command_open",
            command(
                1,
                Op::Open {
                    provider: Provider::Claude,
                    workspace: checkout(),
                },
            ),
        ),
        (
            "command_bootstrap",
            command(
                2,
                Op::Bootstrap {
                    provider: Provider::Codex,
                    model: Some("gpt-5.6".into()),
                    effort: Some("high".into()),
                    workspace: WireChoice::NewWorktree {
                        repo: "/src/ferrite".into(),
                    },
                    prompt: "fix the parser".into(),
                    group: Some(group),
                    new_group_with: None,
                },
            ),
        ),
        (
            "command_bootstrap_new_group",
            command(
                27,
                Op::Bootstrap {
                    provider: Provider::Claude,
                    model: None,
                    effort: None,
                    workspace: checkout(),
                    prompt: "pair with this".into(),
                    group: None,
                    new_group_with: Some(thread),
                },
            ),
        ),
        (
            "command_cancel_bootstrap",
            command(28, Op::CancelBootstrap { request: 27 }),
        ),
        (
            "command_send",
            command(
                3,
                Op::Send {
                    thread,
                    text: "go on".into(),
                },
            ),
        ),
        (
            "command_queue",
            command(
                4,
                Op::Queue {
                    thread,
                    text: "and then the tests".into(),
                },
            ),
        ),
        ("command_unqueue", command(5, Op::Unqueue { thread })),
        (
            "command_cancel_queued",
            command(
                6,
                Op::CancelQueued {
                    thread,
                    restore: true,
                    edit: false,
                },
            ),
        ),
        (
            "command_cancel_queued_edit",
            command(
                29,
                Op::CancelQueued {
                    thread,
                    restore: true,
                    edit: true,
                },
            ),
        ),
        (
            "command_take_recovered",
            command(30, Op::TakeRecovered { thread }),
        ),
        (
            "command_recall",
            command(
                31,
                Op::Recall {
                    thread,
                    direction: WireDirection::Older,
                    draft: "half a thought".into(),
                },
            ),
        ),
        (
            "command_revive_group",
            command(32, Op::ReviveGroup { group }),
        ),
        (
            "command_cli_replaced",
            command(
                33,
                Op::CliReplaced {
                    provider: Provider::Codex,
                },
            ),
        ),
        (
            "command_worktrees_listed",
            command(
                34,
                Op::WorktreesListed {
                    thread,
                    listed: vec!["/src/ferrite".into(), "/src/ferrite-wt/4".into()],
                    age_ms: 120,
                },
            ),
        ),
        (
            "command_load_subject",
            command(
                35,
                Op::LoadSubject {
                    thread,
                    subject: WireSubject::Subagent {
                        key: "claude:root:agent-1".into(),
                    },
                    retry: true,
                },
            ),
        ),
        (
            "command_show_subject",
            command(
                36,
                Op::ShowSubject {
                    thread,
                    subject: WireSubject::Main,
                },
            ),
        ),
        (
            "command_say",
            command(
                37,
                Op::Say {
                    thread,
                    text: "model unchanged: a turn is running".into(),
                },
            ),
        ),
        ("command_interrupt", command(7, Op::Interrupt { thread })),
        (
            "command_answer",
            command(
                8,
                Op::Answer {
                    thread,
                    handle: handle(),
                    answer: WireAnswer::Allow {
                        input: json!({"file_path": "ferrite-perm.txt"}),
                    },
                },
            ),
        ),
        (
            "command_answer_questions",
            command(
                9,
                Op::Answer {
                    thread,
                    handle: handle(),
                    answer: WireAnswer::Questions {
                        answers: vec![WireQuestionAnswer {
                            picks: vec![0],
                            other: Some("both, really".into()),
                        }],
                    },
                },
            ),
        ),
        (
            "command_control",
            command(
                10,
                Op::Control {
                    thread,
                    control: WireControl::ReconnectMcp {
                        server: "github".into(),
                    },
                },
            ),
        ),
        (
            "command_set_model",
            command(
                11,
                Op::SetModel {
                    thread,
                    model: Some("opus".into()),
                },
            ),
        ),
        (
            "command_set_effort",
            command(
                12,
                Op::SetEffort {
                    thread,
                    effort: None,
                },
            ),
        ),
        (
            "command_set_provider",
            command(
                13,
                Op::SetProvider {
                    thread,
                    provider: Provider::Codex,
                    model: None,
                },
            ),
        ),
        (
            "command_rename",
            command(
                14,
                Op::Rename {
                    thread,
                    title: "Parser".into(),
                },
            ),
        ),
        ("command_park", command(15, Op::Park { thread })),
        ("command_revive", command(16, Op::Revive { thread })),
        ("command_delete", command(17, Op::Delete { thread })),
        (
            "command_import",
            command(
                18,
                Op::Import {
                    path: "/tmp/session.jsonl".into(),
                },
            ),
        ),
        (
            "command_group",
            command(
                19,
                Op::Group {
                    change: WireGroupChange::Join {
                        thread,
                        group,
                        index: Some(0),
                    },
                },
            ),
        ),
        (
            "command_layout",
            command(
                20,
                Op::Layout {
                    group,
                    tree: Some(crate::layout::Tree {
                        root: Some(crate::layout::Node::Leaf(thread)),
                    }),
                },
            ),
        ),
        (
            "command_project",
            command(
                21,
                Op::Project {
                    action: ProjectAction::Rename {
                        project,
                        title: "Ferrite".into(),
                    },
                },
            ),
        ),
        (
            "command_notice",
            command(
                22,
                Op::Notice {
                    action: NoticeAction::DismissDecision {
                        thread,
                        handle: handle(),
                    },
                },
            ),
        ),
        ("command_settings_changed", command(23, Op::SettingsChanged)),
        (
            "command_search_files",
            command(
                24,
                Op::SearchFiles {
                    thread,
                    query: "pars".into(),
                },
            ),
        ),
        (
            "command_restart",
            command(25, Op::Restart { when: When::Idle }),
        ),
        ("command_stop", command(26, Op::Stop { when: When::Now })),
        (
            "command_coordinate",
            command(
                27,
                Op::Coordinate {
                    project,
                    provider: Provider::Codex,
                },
            ),
        ),
        (
            "command_list_dir",
            command(
                38,
                Op::ListDir {
                    path: "~/src".into(),
                },
            ),
        ),
        ("command_list_repo", command(39, Op::ListRepo { project })),
        (
            "welcome",
            Sample::Server(ServerMessage::Welcome {
                generation: 1,
                server: ServerInfo {
                    version: "0.9.0".into(),
                    executable: "/Applications/Ferrite.app/Contents/MacOS/ferrite".into(),
                },
                boot_id: "b-1".into(),
                capabilities: vec![],
                error: None,
                uptime_ms: None,
            }),
        ),
        (
            "welcome_uptime",
            Sample::Server(ServerMessage::Welcome {
                generation: 1,
                server: ServerInfo {
                    version: "0.9.0".into(),
                    executable: "/Applications/Ferrite.app/Contents/MacOS/ferrite".into(),
                },
                boot_id: "b-1".into(),
                capabilities: vec![],
                error: None,
                uptime_ms: Some(61_000),
            }),
        ),
        (
            "snapshot",
            Sample::Server(ServerMessage::Snapshot(Snapshot {
                boot_id: "b-1".into(),
                revision: 7,
                threads: vec![state()],
                roster: SharedRoster {
                    open: vec![thread],
                    parked: vec![ThreadId::new(2)],
                },
                decisions: vec![PendingEntry {
                    thread,
                    subject: None,
                    handle: handle(),
                    decision: decision(),
                }],
                groups: Some(json!({"groups": [], "version": 1})),
                registry: None,
                notices: Some(vec![WireNotice::Finished {
                    id: 1,
                    thread,
                    outcome: WireOutcome::Completed,
                    at_ms: 1_760_000_000_000,
                    read: false,
                }]),
                providers: Some(vec![ProviderState {
                    provider: Provider::Claude,
                    live_sessions: 1,
                    models: vec![],
                    limits: Some(WireRateLimits {
                        five_hour: Some(WireRateWindow {
                            used_fraction: 0.5,
                            resets_at: Some(1_760_003_600),
                        }),
                        weekly: None,
                    }),
                }]),
                parked: None,
            })),
        ),
        (
            "snapshot_parked",
            Sample::Server(ServerMessage::Snapshot(Snapshot {
                boot_id: "b-1".into(),
                revision: 7,
                threads: vec![],
                roster: SharedRoster {
                    open: vec![],
                    parked: vec![ThreadId::new(2)],
                },
                decisions: vec![],
                groups: None,
                registry: None,
                notices: None,
                providers: None,
                parked: Some(vec![parked_row()]),
            })),
        ),
        (
            "history",
            Sample::Server(ServerMessage::History {
                thread,
                after: 0,
                inputs: vec![
                    WireInput::Replay(record(Input::Prompt("fix the parser".into()))),
                    WireInput::Replay(record(Input::Event(SessionEvent::TextDelta {
                        text: "On it.".into(),
                    }))),
                ],
            }),
        ),
        (
            "history_end",
            Sample::Server(ServerMessage::HistoryEnd {
                thread,
                seq: 41,
                live: vec![
                    WireInput::Live(Live::Connect { generation: 3 }),
                    WireInput::Live(Live::DecisionRequested {
                        decision: decision(),
                    }),
                ],
            }),
        ),
        (
            "decision",
            Sample::Server(ServerMessage::Decision {
                thread,
                subject: Some(WireSubject::Main),
                handle: handle(),
                decision: question_decision(),
            }),
        ),
        (
            "decision_closed",
            Sample::Server(ServerMessage::DecisionClosed {
                thread,
                handle: handle(),
                by: ClosedBy::Answered { client: 2 },
            }),
        ),
        (
            "decision_closed_restart",
            Sample::Server(ServerMessage::DecisionClosed {
                thread,
                handle: handle(),
                by: ClosedBy::ServerRestart,
            }),
        ),
        (
            "reply_ok",
            Sample::Server(ServerMessage::Reply {
                id: 1,
                ok: Some(Done::Thread { thread }),
                error: None,
            }),
        ),
        (
            "reply_files",
            Sample::Server(ServerMessage::Reply {
                id: 24,
                ok: Some(Done::Files {
                    files: vec![WireFile {
                        path: "src/parser.rs".into(),
                        is_directory: false,
                        matched: vec![4, 5, 6, 7],
                    }],
                }),
                error: None,
            }),
        ),
        (
            "reply_listing",
            Sample::Server(ServerMessage::Reply {
                id: 38,
                ok: Some(Done::Listing {
                    path: "/home/me/src".into(),
                    entries: vec![
                        WireEntry {
                            name: "ferrite".into(),
                            directory: true,
                        },
                        WireEntry {
                            name: "notes.md".into(),
                            directory: false,
                        },
                    ],
                }),
                error: None,
            }),
        ),
        (
            "reply_repo_listing",
            Sample::Server(ServerMessage::Reply {
                id: 39,
                ok: Some(Done::RepoListing {
                    checked_out: Some("main".into()),
                    worktrees: vec![
                        WireWorktree {
                            path: "/home/me/src/ferrite/.worktrees/fix".into(),
                            branch: Some("fix".into()),
                        },
                        WireWorktree {
                            path: "/home/me/src/ferrite/.worktrees/bisect".into(),
                            branch: None,
                        },
                    ],
                    branches: vec!["main".into(), "fix".into()],
                }),
                error: None,
            }),
        ),
        (
            "reply_text",
            Sample::Server(ServerMessage::Reply {
                id: 5,
                ok: Some(Done::Text {
                    text: "and then the tests".into(),
                }),
                error: None,
            }),
        ),
        (
            "reply_error",
            Sample::Server(ServerMessage::Reply {
                id: 8,
                ok: None,
                error: Some(WireError {
                    code: ErrorCode::AlreadyAnswered,
                    message: "another window answered first".into(),
                }),
            }),
        ),
        (
            "thread",
            Sample::Server(ServerMessage::Thread {
                revision: 8,
                state: state(),
            }),
        ),
        (
            "thread_starting",
            Sample::Server(ServerMessage::Thread {
                revision: 15,
                state: ThreadState {
                    for_draft: true,
                    recovered: vec![WireRecovered {
                        text: "and then the tests".into(),
                        prepend: true,
                    }],
                    wants_listing: true,
                    history_errors: vec![WireHistoryError {
                        agent: "claude:root:agent-1".into(),
                        error: "the log is gone".into(),
                    }],
                    ..state()
                },
            }),
        ),
        (
            "allowance_warning",
            Sample::Server(ServerMessage::AllowanceWarning {
                provider: Provider::Claude,
                window: WireLimitWindow::FiveHour,
                threshold: 80,
                used_fraction: 0.5,
            }),
        ),
        (
            "reply_changed",
            Sample::Server(ServerMessage::Reply {
                id: 34,
                ok: Some(Done::Changed { changed: true }),
                error: None,
            }),
        ),
        (
            "reply_threads",
            Sample::Server(ServerMessage::Reply {
                id: 32,
                ok: Some(Done::Threads {
                    threads: vec![thread, ThreadId::new(2)],
                }),
                error: None,
            }),
        ),
        (
            "reply_recovered",
            Sample::Server(ServerMessage::Reply {
                id: 30,
                ok: Some(Done::Recovered {
                    text: "and then the tests".into(),
                    prepend: false,
                }),
                error: None,
            }),
        ),
        (
            "reply_grouped",
            Sample::Server(ServerMessage::Reply {
                id: 19,
                ok: Some(Done::Grouped {
                    group: Some(group),
                    dissolved: vec![WireDissolved {
                        group: GroupId::new(2),
                        survivor: ThreadId::new(2),
                    }],
                }),
                error: None,
            }),
        ),
        (
            "tool_draft",
            Sample::Server(ServerMessage::ToolDraft {
                thread,
                id: "toolu_09".into(),
                name: "show_visual".into(),
                input: json!({"html": "<div>", "title": "Chart"}),
            }),
        ),
        (
            "roster",
            Sample::Server(ServerMessage::Roster {
                revision: 9,
                roster: SharedRoster {
                    open: vec![thread, ThreadId::new(2)],
                    parked: vec![],
                },
            }),
        ),
        (
            "groups",
            Sample::Server(ServerMessage::Groups {
                revision: 10,
                groups: json!({"groups": [], "version": 1}),
            }),
        ),
        (
            "registry",
            Sample::Server(ServerMessage::Registry {
                revision: 11,
                registry: json!({"projects": []}),
            }),
        ),
        (
            "notices",
            Sample::Server(ServerMessage::Notices {
                revision: 12,
                notices: vec![WireNotice::Request {
                    thread,
                    handle: handle(),
                    subject: None,
                    question: false,
                    read: false,
                    at_ms: 1_760_000_000_000,
                    seq: 1,
                    dismissed: false,
                }],
            }),
        ),
        (
            "notices_dismissed",
            Sample::Server(ServerMessage::Notices {
                revision: 14,
                notices: vec![WireNotice::Request {
                    thread,
                    handle: handle(),
                    subject: None,
                    question: true,
                    read: true,
                    at_ms: 1_760_000_000_000,
                    seq: 2,
                    dismissed: true,
                }],
            }),
        ),
        (
            "providers",
            Sample::Server(ServerMessage::Providers {
                revision: 13,
                providers: vec![ProviderState {
                    provider: Provider::Codex,
                    live_sessions: 0,
                    models: vec![model("gpt-5.6")],
                    limits: None,
                }],
            }),
        ),
        (
            "parked",
            Sample::Server(ServerMessage::Parked {
                revision: 14,
                threads: vec![
                    parked_row(),
                    ParkedThread {
                        thread: ThreadId::new(3),
                        provider: None,
                        title: None,
                        prompt_title: None,
                        workspace: None,
                        session_project_root: None,
                        project: None,
                        last_used_ms: Some(1_760_000_000_000),
                        turns: None,
                        subagents: None,
                    },
                ],
            }),
        ),
        (
            "render",
            Sample::Server(ServerMessage::Render {
                request: 9,
                id: "toolu_09".into(),
                title: "Chart".into(),
                html: "<div>chart</div>".into(),
            }),
        ),
        (
            "restarting",
            Sample::Server(ServerMessage::Restarting { when: When::Idle }),
        ),
        ("stopping", Sample::Server(ServerMessage::Stopping)),
        (
            "focus",
            Sample::Server(ServerMessage::Focus {
                thread: ThreadId::new(7),
            }),
        ),
        (
            "notification",
            Sample::Server(ServerMessage::Notification {
                title: "Review ready".into(),
                body: Some("3 findings".into()),
            }),
        ),
        (
            "notification_title_only",
            Sample::Server(ServerMessage::Notification {
                title: "Review ready".into(),
                body: None,
            }),
        ),
        (
            "input_record_text",
            input(
                1,
                WireInput::Record(record(Input::Event(SessionEvent::TextDelta {
                    text: "Reading the parser".into(),
                }))),
            ),
        ),
        (
            "input_record_prompt",
            input(2, WireInput::Record(record(Input::Prompt("go on".into())))),
        ),
        (
            "input_record_subagent",
            input(
                3,
                WireInput::Record(record(Input::Event(SessionEvent::Activity(
                    ActivityEvent::Content {
                        key: AgentKey::from_stored("[\"claude\",\"root\",\"a1\"]".into()),
                        id: Some("m1".into()),
                        event: ExecutionEvent::TextDelta {
                            text: "child text".into(),
                        },
                    },
                )))),
            ),
        ),
        (
            "input_replay",
            input(
                4,
                WireInput::Replay(record(Input::Event(SessionEvent::TurnEnded {
                    outcome: crate::TurnOutcome::Completed,
                    cost_usd: Some(0.04),
                }))),
            ),
        ),
        (
            "live_notice",
            live(
                5,
                Live::Notice {
                    text: "send failed: broken pipe".into(),
                    replayed: false,
                },
            ),
        ),
        (
            "live_notice_replayed",
            live(
                24,
                Live::Notice {
                    text: "continued on Codex · default".into(),
                    replayed: true,
                },
            ),
        ),
        ("live_revived", live(6, Live::Revived)),
        ("live_connect", live(7, Live::Connect { generation: 3 })),
        ("live_disconnect", live(8, Live::Disconnect)),
        (
            "live_answer_submitted",
            live(9, Live::AnswerSubmitted { handle: handle() }),
        ),
        (
            "live_answered",
            live(
                10,
                Live::Answered {
                    handle: handle(),
                    allowed: true,
                },
            ),
        ),
        (
            "live_decision_requested",
            live(
                11,
                Live::DecisionRequested {
                    decision: decision(),
                },
            ),
        ),
        (
            "live_decision",
            live(
                12,
                Live::Decision {
                    subject: Some(WireSubject::Subagent {
                        key: "[\"codex\",\"root\",\"t2\"]".into(),
                    }),
                    decision: form_decision(),
                },
            ),
        ),
        (
            "live_decision_reply",
            live(
                13,
                Live::DecisionReply {
                    id: "q_01".into(),
                    error: Some("rejected".into()),
                },
            ),
        ),
        (
            "live_decision_cancelled",
            live(
                14,
                Live::DecisionCancelled {
                    id: "perm_01".into(),
                },
            ),
        ),
        (
            "live_commands",
            live(
                15,
                Live::Commands {
                    commands: vec![WireCommand {
                        name: "skill".into(),
                        description: "A skill".into(),
                        path: Some("/skills/a/SKILL.md".into()),
                    }],
                },
            ),
        ),
        (
            "live_permission_mode",
            live(
                16,
                Live::PermissionMode {
                    mode: "acceptEdits".into(),
                },
            ),
        ),
        (
            "live_models",
            live(
                17,
                Live::Models {
                    models: vec![model("opus")],
                },
            ),
        ),
        (
            "live_rate_limits",
            live(
                18,
                Live::RateLimits {
                    limits: WireRateLimits {
                        five_hour: None,
                        weekly: Some(WireRateWindow {
                            used_fraction: 0.25,
                            resets_at: None,
                        }),
                    },
                },
            ),
        ),
        (
            "live_mcp_servers",
            live(
                19,
                Live::McpServers {
                    servers: vec![WireMcpServer {
                        name: "github".into(),
                        status: WireMcpStatus::NeedsAuth,
                        error: None,
                    }],
                },
            ),
        ),
        (
            "live_mcp_authorization",
            live(
                20,
                Live::McpAuthorization {
                    server: "github".into(),
                    url: Some("https://example.com/auth".into()),
                },
            ),
        ),
        (
            "live_restore_timings",
            live(
                21,
                Live::RestoreTimings {
                    subject: WireSubject::Main,
                    timings: [("toolu_01".to_string(), 1234)].into_iter().collect(),
                },
            ),
        ),
        (
            "live_retain",
            live(
                22,
                Live::Retain {
                    subject: WireSubject::Main,
                },
            ),
        ),
        (
            "live_evict",
            live(
                23,
                Live::Evict {
                    subject: WireSubject::Subagent {
                        key: "[\"claude\",\"root\",\"a1\"]".into(),
                    },
                },
            ),
        ),
    ]
}

#[test]
fn every_gen1_message_has_a_frozen_fixture_that_round_trips_byte_for_byte() {
    let dir = fixtures("gen1");
    let bless = std::env::var_os("FERRITE_BLESS_FIXTURES").is_some();
    let mut missing = Vec::new();
    for (name, sample) in samples() {
        let path = dir.join(format!("{name}.json"));
        let encoded = sample.encode();
        let Ok(frozen) = std::fs::read(&path) else {
            if bless {
                std::fs::write(&path, &encoded).unwrap();
            } else {
                missing.push(name);
            }
            continue;
        };
        assert_eq!(
            String::from_utf8_lossy(&encoded),
            String::from_utf8_lossy(&frozen),
            "{name}: this build no longer encodes the frozen fixture"
        );
        assert!(
            sample.decodes_from(&frozen),
            "{name} decodes to something else"
        );
    }
    assert!(
        missing.is_empty(),
        "no fixture for {missing:?}: run with FERRITE_BLESS_FIXTURES=1"
    );
}

/// A fixture outlives the sample that wrote it: every one on disk still
/// decodes and re-encodes as it was frozen.
#[test]
fn every_frozen_fixture_still_decodes() {
    for entry in std::fs::read_dir(fixtures("gen1")).unwrap() {
        let path = entry.unwrap().path();
        let frozen = std::fs::read(&path).unwrap();
        let line = frozen.strip_suffix(b"\n").unwrap();
        let reencoded = if decode::<ClientMessage>(line)
            .is_ok_and(|message| message != ClientMessage::Unknown)
        {
            encode(&decode::<ClientMessage>(line).unwrap())
        } else {
            let message = decode::<ServerMessage>(line).unwrap();
            assert_ne!(message, ServerMessage::Unknown, "{path:?}");
            encode(&message)
        };
        assert_eq!(reencoded, frozen, "{path:?}");
    }
}

fn tolerance(name: &str) -> Vec<u8> {
    let bytes = std::fs::read(fixtures("tolerance").join(format!("{name}.json"))).unwrap();
    bytes.strip_suffix(b"\n").unwrap().to_vec()
}

#[test]
fn unknown_fields_are_ignored() {
    let hello: ClientMessage = decode(&tolerance("unknown_field")).unwrap();
    assert!(matches!(hello, ClientMessage::Hello { generation: 1, .. }));
}

#[test]
fn an_unknown_enum_value_decodes_to_unknown() {
    let ClientMessage::Hello { capabilities, .. } = decode(&tolerance("unknown_enum")).unwrap()
    else {
        panic!("a hello");
    };
    assert_eq!(
        capabilities,
        vec![Capability::RenderVisual, Capability::Unknown]
    );
}

#[test]
fn an_unknown_message_type_is_ignored() {
    let message: ServerMessage = decode(&tolerance("unknown_message")).unwrap();
    assert_eq!(message, ServerMessage::Unknown);
    let message: ClientMessage = decode(&tolerance("unknown_message")).unwrap();
    assert_eq!(message, ClientMessage::Unknown);
}

#[test]
fn an_unknown_command_keeps_its_id_to_be_answered_unsupported() {
    let message: ClientMessage = decode(&tolerance("unknown_command")).unwrap();
    assert_eq!(
        message,
        ClientMessage::Command(Command {
            id: 77,
            op: Op::Unknown
        })
    );
}

#[test]
fn absent_optional_fields_take_their_defaults() {
    let ServerMessage::Welcome {
        capabilities,
        error,
        ..
    } = decode(&tolerance("absent_optional")).unwrap()
    else {
        panic!("a welcome");
    };
    assert!(capabilities.is_empty());
    assert_eq!(error, None);
}

#[test]
fn a_newer_generation_is_refused_with_unsupported_generation() {
    let ClientMessage::Hello { generation, .. } = decode(&tolerance("newer_generation")).unwrap()
    else {
        panic!("a hello");
    };
    let server = ServerInfo {
        version: "0.9.0".into(),
        executable: "/bin/ferrite".into(),
    };
    let ServerMessage::Welcome { error, .. } = welcome(generation, server.clone(), "b-1".into(), 0)
    else {
        panic!("a welcome");
    };
    assert_eq!(error.unwrap().code, ErrorCode::UnsupportedGeneration);
    let ServerMessage::Welcome { error, .. } = welcome(GENERATION, server, "b-1".into(), 0) else {
        panic!("a welcome");
    };
    assert_eq!(error, None);
}

/// A record kind from a newer Store schema is skipped by an older window,
/// not a reason to drop the connection.
#[test]
fn an_unknown_record_kind_decodes_and_is_skipped() {
    let ServerMessage::Input { input, .. } = decode(&tolerance("unknown_record")).unwrap() else {
        panic!("an input");
    };
    assert_eq!(input, WireInput::Record(WireRecord::Unknown));
    assert_eq!(incoming(&input, 1, crate::clock::instant()), None);
}

#[test]
fn an_unknown_live_kind_decodes_and_is_skipped() {
    let ServerMessage::HistoryEnd { live, .. } = decode(&tolerance("unknown_live")).unwrap() else {
        panic!("a history end");
    };
    assert_eq!(live, vec![WireInput::Live(Live::Unknown)]);
    assert_eq!(incoming(&live[0], 1, crate::clock::instant()), None);
}

/// Every input kind the Engine applies comes back out of the wire as the
/// same input: what lets a mirror equal the Engine.
#[test]
fn applied_inputs_survive_the_wire() {
    let at = crate::clock::instant();
    let core_decision = crate::Decision::try_from(&question_decision()).unwrap();
    let subject = Subject::Subagent(AgentKey::from_stored("[\"claude\",\"r\",\"a\"]".into()));
    let main = |event| ActivityInput::Main {
        input: Input::Event(event),
        at,
    };
    let observe = |event| ActivityInput::Observe {
        generation: 3,
        event,
        at,
    };
    let cases = vec![
        main(SessionEvent::TextDelta { text: "hi".into() }),
        ActivityInput::Main {
            input: Input::Prompt("go".into()),
            at,
        },
        ActivityInput::Main {
            input: Input::Notice("note".into()),
            at,
        },
        ActivityInput::Main {
            input: Input::Revived,
            at,
        },
        main(SessionEvent::DecisionRequested {
            decision: core_decision.clone(),
        }),
        main(SessionEvent::Commands {
            commands: vec![crate::SessionCommand {
                name: "review".into(),
                description: "Review".into(),
                path: None,
            }],
        }),
        main(SessionEvent::PermissionMode {
            mode: "plan".into(),
        }),
        main(SessionEvent::Models {
            models: vec![model("opus")],
        }),
        main(SessionEvent::RateLimits {
            five_hour: Some(crate::RateLimitWindow {
                used_fraction: 0.5,
                resets_at: Some(9),
            }),
            weekly: None,
        }),
        main(SessionEvent::McpServers {
            servers: vec![crate::McpServer {
                name: "github".into(),
                status: crate::McpStatus::Failed,
                error: Some("refused".into()),
            }],
        }),
        main(SessionEvent::McpAuthorization {
            server: "github".into(),
            url: None,
        }),
        observe(ActivityEvent::Content {
            key: AgentKey::from_stored("[\"claude\",\"r\",\"a\"]".into()),
            id: None,
            event: ExecutionEvent::TextDelta {
                text: "child".into(),
            },
        }),
        observe(ActivityEvent::Decision {
            subject: Some(subject.clone()),
            decision: core_decision,
        }),
        observe(ActivityEvent::DecisionReply {
            id: "q".into(),
            error: None,
        }),
        observe(ActivityEvent::DecisionCancelled { id: "q".into() }),
        ActivityInput::Replay(Input::Prompt("before".into())),
        ActivityInput::Replay(Input::Notice("continued on Codex".into())),
        ActivityInput::ReplayEvent(ActivityEvent::Status {
            key: AgentKey::from_stored("[\"claude\",\"r\",\"a\"]".into()),
            state: crate::activity::AgentStatus::Idle,
        }),
        ActivityInput::Connect { generation: 3 },
        ActivityInput::Disconnect,
        ActivityInput::AnswerSubmitted {
            handle: (&handle()).into(),
        },
        ActivityInput::Answered {
            handle: (&handle()).into(),
            allowed: false,
            at,
        },
        ActivityInput::RestoreTimings {
            subject: subject.clone(),
            timings: [("t".to_string(), Duration::from_millis(1500))]
                .into_iter()
                .collect(),
        },
        ActivityInput::Retain(subject.clone()),
        ActivityInput::Evict(subject),
    ];
    for case in cases {
        let Outgoing::Input(wire) = outgoing(&case, None) else {
            panic!("{case:?} crosses as an input");
        };
        let line = encode(&wire);
        let wire: WireInput = decode(line.strip_suffix(b"\n").unwrap()).unwrap();
        assert_eq!(incoming(&wire, 3, at).as_ref(), Some(&case), "{wire:?}");
    }
}

#[test]
fn a_tool_draft_crosses_as_its_own_latest_wins_message() {
    let draft = ActivityInput::Main {
        input: Input::Event(SessionEvent::ToolDraft {
            id: "toolu_09".into(),
            name: "show_visual".into(),
            input: json!({"html": "<p"}),
        }),
        at: crate::clock::instant(),
    };
    assert_eq!(
        outgoing(&draft, None),
        Outgoing::ToolDraft {
            id: "toolu_09".into(),
            name: "show_visual".into(),
            input: json!({"html": "<p"}),
        }
    );
    assert_eq!(
        outgoing(&ActivityInput::DrainHighlights, None),
        Outgoing::Local
    );
}

#[test]
fn answers_and_decisions_convert_both_ways() {
    for decision in [decision(), question_decision(), form_decision()] {
        let core = crate::Decision::try_from(&decision).unwrap();
        assert_eq!(WireDecision::from(&core), decision);
    }
    for answer in [
        WireAnswer::Allow { input: json!({}) },
        WireAnswer::Deny {
            message: "no".into(),
        },
        WireAnswer::AllowAlways {
            input: json!({}),
            suggestion: json!("rule"),
        },
        WireAnswer::Choose { value: json!(2) },
        WireAnswer::Questions {
            answers: vec![WireQuestionAnswer {
                picks: vec![1],
                other: None,
            }],
        },
        WireAnswer::Form {
            values: json!({"name": "x"}),
        },
        WireAnswer::Cancel,
    ] {
        let core = crate::DecisionAnswer::try_from(&answer).unwrap();
        assert_eq!(WireAnswer::from(&core), answer);
    }
    assert!(crate::DecisionAnswer::try_from(&WireAnswer::Unknown).is_err());
}

/// A Decision of a kind this window does not know is still one it can
/// see and refuse.
#[test]
fn an_unknown_decision_kind_reads_as_one_this_window_cannot_draw() {
    let decision = WireDecision {
        kind: WireDecisionKind::Unknown,
        ..decision()
    };
    let core = crate::Decision::try_from(&decision).unwrap();
    assert!(matches!(core.kind, crate::DecisionKind::Unsupported { .. }));
}
