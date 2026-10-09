//! Native framebuffer artifacts using the real Cockpit renderer.
//! No live providers or operator store. Opt in with `visual-reference`.
//!
//! Every state builds its own Cockpit on a disposable store under the
//! system temp directory, feeds fixture Sessions the events a real provider
//! would, and saves `{state}-{size}.png`. `FERRITE_REF_STATES` and
//! `FERRITE_REF_SIZES` (comma lists) narrow a run to a subset.
//!
//! The `parity-*` states rebuild the approved terminal-native prototype's
//! views on one shared world (`parity_scenes.rs`); run them with
//! `--features visual-reference,opaque` so paints are the opaque greys the
//! prototype's shots use.

#[path = "cockpit/visual_reference/parity_scenes.rs"]
mod parity_scenes;

#[path = "cockpit/visual_reference/live.rs"]
mod live;
pub(crate) use live::capture as live_capture;

use super::{CockpitView, DraftTarget, MenuTarget};
use crate::nav::NavFold;
use crate::palette::PaletteScope;
use ferrite_core::{
    activity::TranscriptCoverage,
    activity::{ActivityEvent, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject},
    cockpit::{Cockpit, SpawnRequest, Spawner},
    groups::{GroupChange, GroupId},
    layout::{Axis, Node, Tree},
    providers::Session,
    settings::ReadingSize,
    store::{Provider, Store},
    workspace::WorkspaceChoice,
    Decision, DecisionAnswer, DecisionKind, Hunk, QueueEvent, QueuedPrompt, SessionCommand,
    SessionEvent, ThreadId, ToolResult, TurnOutcome,
};
use gpui::{AppContext, Context, HeadlessAppContext, Window};
use std::{
    cell::RefCell,
    io,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
};

type Feeds = Rc<RefCell<Vec<mpsc::Sender<SessionEvent>>>>;

/// Hands every spawn a fresh channel; the scene keeps the sending half.
struct Fixture(Feeds);
struct FixtureSession {
    rx: mpsc::Receiver<SessionEvent>,
    tx: mpsc::Sender<SessionEvent>,
}
impl Spawner for Fixture {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn Session>> {
        let (tx, rx) = mpsc::channel();
        self.0.borrow_mut().push(tx.clone());
        Ok(Box::new(FixtureSession { rx, tx }))
    }
}
impl Session for FixtureSession {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        &self.rx
    }
    fn send(&mut self, _: &str) -> io::Result<()> {
        Ok(())
    }
    /// The provider accepts every held prompt, as Claude's native queue does.
    fn enqueue(&mut self, id: &str, text: &str) -> io::Result<()> {
        let _ = self
            .tx
            .send(SessionEvent::Queue(QueueEvent::Accepted(QueuedPrompt {
                id: id.into(),
                client_id: id.into(),
                text: text.into(),
            })));
        Ok(())
    }
    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> io::Result<()> {
        Ok(())
    }
}

/// One Thread's provider side: what it says, in order.
struct Feed(mpsc::Sender<SessionEvent>);
impl Feed {
    fn ev(&self, event: SessionEvent) -> &Self {
        self.0.send(event).expect("fixture Session is listening");
        self
    }
    fn boot(&self, provider: Provider, tokens: u64) -> &Self {
        let model = match provider {
            Provider::Claude => "claude-opus-5-5[1m]",
            Provider::Codex => "gpt-6-sol",
        };
        self.ev(SessionEvent::Init {
            session_id: format!("fixture-{tokens}"),
            model: model.into(),
        })
        .ev(SessionEvent::PermissionMode {
            mode: "acceptEdits".into(),
        })
        .ev(SessionEvent::TokenUsage {
            total_tokens: tokens,
            input_tokens: tokens,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            context_window: Some(200_000),
        })
    }
    fn text(&self, text: &str) -> &Self {
        self.ev(SessionEvent::TextDelta { text: text.into() })
    }
    fn tool(&self, id: &str, name: &str, input: serde_json::Value) -> &Self {
        self.ev(SessionEvent::ToolStarted {
            id: id.into(),
            name: name.into(),
            input,
        })
    }
    fn done(&self, id: &str, output: &str, is_error: bool, result: ToolResult) -> &Self {
        self.ev(SessionEvent::ToolCompleted {
            id: id.into(),
            output: output.into(),
            is_error,
            result,
        })
    }
    fn ok(&self, id: &str, output: &str) -> &Self {
        self.done(id, output, false, ToolResult::Opaque)
    }
    fn bash(&self, id: &str, command: &str, stdout: &str, exit: i64, ms: u64) -> &Self {
        self.tool(id, "Bash", serde_json::json!({ "command": command }))
            .done(
                id,
                stdout,
                exit != 0,
                ToolResult::Command {
                    stdout: stdout.into(),
                    stderr: String::new(),
                    exit_code: Some(exit),
                    duration_ms: Some(ms),
                },
            )
    }
    fn edit(&self, id: &str, path: &str, hunk: Hunk) -> &Self {
        self.tool(id, "Edit", serde_json::json!({ "file_path": path }))
            .done(
                id,
                "applied",
                false,
                ToolResult::FileEdit {
                    path: path.into(),
                    hunks: vec![hunk],
                },
            )
    }
    fn end(&self, cost: f64) -> &Self {
        self.ev(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(cost),
        })
    }
    fn approval(&self, id: &str, command: &str) -> &Self {
        self.tool(id, "Bash", serde_json::json!({ "command": command }))
            .ev(SessionEvent::DecisionRequested {
                decision: Decision {
                    delivery: Default::default(),
                    kind: DecisionKind::Approval,
                    policy: Default::default(),
                    id: format!("{id}-decision"),
                    tool_use_id: id.into(),
                    tool_name: "Bash".into(),
                    description: command.into(),
                    suggestions: vec![],
                    input: serde_json::json!({ "command": command }),
                },
            })
    }
    fn agent(&self, event: ActivityEvent) -> &Self {
        self.ev(SessionEvent::Activity(event))
    }
}

/// One capture's world: a Cockpit on a disposable store, its fixture
/// Sessions, and the Project directories the Threads bind.
struct Scene {
    core: Cockpit,
    feeds: Feeds,
    root: PathBuf,
    /// What must outlive the capture and go with it: a fixture clock, the
    /// HOME a scene pointed at its own root. Dropped after the screenshot.
    hold: Vec<Box<dyn std::any::Any>>,
}

type Setup = Box<dyn FnOnce(&mut CockpitView, &mut Window, &mut Context<CockpitView>)>;

impl Scene {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("ferrite-reference-{}-{tag}", std::process::id()));
        // Only our own disposable directory; never the operator's store.
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create disposable reference root");
        let feeds = Feeds::default();
        let mut core = Cockpit::new(
            Store::open(root.join("store")).expect("open disposable store"),
            Box::new(Fixture(feeds.clone())),
        );
        core.set_suggestions_enabled(false);
        Self {
            core,
            feeds,
            root,
            hold: Vec::new(),
        }
    }

    /// A Project directory named `name` (its leaf is the Project's title),
    /// made a git checkout so the nav has a branch to name.
    fn project(&self, name: &str) -> PathBuf {
        let dir = self.root.join("work").join(name);
        if !dir.exists() {
            std::fs::create_dir_all(&dir).expect("create fixture project");
            let _ = std::process::Command::new("git")
                .args(["init", "-q", "-b", "main"])
                .current_dir(&dir)
                .status();
        }
        dir
    }

    fn open(&mut self, provider: Provider, checkout: &Path, title: &str) -> (ThreadId, Feed) {
        let thread = self
            .core
            .open(
                provider,
                WorkspaceChoice::Main {
                    checkout: checkout.to_path_buf(),
                },
            )
            .expect("open fixture Thread");
        if !title.is_empty() {
            self.core
                .rename_thread(thread, title)
                .expect("name fixture Thread");
        }
        let feed = Feed(self.feeds.borrow().last().expect("spawned").clone());
        (thread, feed)
    }

    fn group(&mut self, members: &[ThreadId], title: &str) -> GroupId {
        let group = self
            .core
            .apply_group(GroupChange::Create {
                first: members[0],
                second: members[1],
            })
            .expect("group fixture Threads")
            .group
            .expect("a new Group");
        for thread in &members[2..] {
            self.core
                .apply_group(GroupChange::Join {
                    thread: *thread,
                    group,
                    index: None,
                })
                .expect("join fixture Group");
        }
        self.core
            .apply_group(GroupChange::Rename {
                group,
                title: title.into(),
            })
            .expect("name fixture Group");
        group
    }
}

/// Sizes by label: the two transcript widths the references began with,
/// and the real default window.
const SIZES: [(&str, f32, f32); 3] = [
    ("narrow", 720., 1400.),
    ("wide", 1000., 1400.),
    ("app", 1440., 900.),
];

/// Every state, with the sizes it renders at by default.
const STATES: &[(&str, &[&str])] = &[
    ("formatting", &["narrow", "wide", "app"]),
    ("edges", &["narrow", "wide", "app"]),
    ("live", &["narrow", "wide", "app"]),
    ("decision", &["narrow", "wide", "app"]),
    ("approval", &["narrow", "wide", "app"]),
    ("expanded", &["narrow", "wide", "app"]),
    ("interrupted", &["narrow", "wide", "app"]),
    ("conversation", &["app", "wide"]),
    ("diff", &["app"]),
    ("nav", &["app"]),
    ("group4", &["app"]),
    ("group9", &["app"]),
    ("group12", &["app"]),
    ("group12-dragged", &["app"]),
    ("subagents", &["app"]),
    ("subagent", &["app"]),
    ("composer", &["app"]),
    ("menu", &["app"]),
    ("modelpicker", &["app"]),
    ("contextmenu", &["app"]),
    ("settings", &["app"]),
    ("draft", &["app"]),
    ("projecteditor", &["app"]),
    ("notifications", &["app"]),
    ("empty-transcript", &["app"]),
    ("starting", &["app"]),
    ("error-turn", &["wide", "app"]),
    ("toolgroups", &["app", "narrow"]),
    ("toolgroups-open", &["app", "narrow"]),
    // ---- WP-A states (append above the end line)
    // (end WP-A)

    // ---- WP-B states (append above the end line)
    ("prose", &["narrow", "wide", "app"]),
    ("prose-comfortable", &["app"]),
    ("prose-large", &["app", "narrow"]),
    ("formatting-large", &["app"]),
    // (end WP-B)

    // ---- WP-C states (append above the end line)
    ("chrome", &["narrow", "wide", "app"]),
    ("checkscard", &["app"]),
    ("emptyboard", &["app"]),
    // (end WP-C)

    // ---- WP-D states (append above the end line)
    ("usagecard", &["app"]),
    ("filescard", &["app"]),
    ("sessioncard", &["app"]),
    ("preview", &["app"]),
    // (end WP-D)

    // ---- WP-E states (append above the end line)
    ("projectcreator", &["app"]),
    ("toasts", &["app"]),
    ("settings-permissions", &["app"]),
    ("settings-behaviour", &["app"]),
    ("settings-about", &["app"]),
    ("settings-search", &["app"]),
    // (end WP-E)

    // ---- WP-F states (append above the end line)
    // (end WP-F)

    // ---- visuals (ADR 0013): agents' pages inline. With `--features cef`
    // (run from the bundle) the pages are Chromium's; without, the fallback.
    ("visuals", &["app", "wide", "narrow"]),
    ("visuals-top", &["app"]),
    ("visuals-group", &["app"]),
    // (end visuals)

    // ---- WP-G states (append above the end line)
    ("navrail", &["app"]),
    ("palettefilter", &["app"]),
    // (end WP-G)

    // ---- parity: the approved prototype's views, one shared world
    // (`parity_scenes.rs`); compared against /tmp/ferrite-tn-proto/<view>.png
    ("parity-solo", &["app"]),
    ("parity-solo-top", &["app"]),
    ("parity-solo-hover", &["app"]),
    ("parity-solo-picker", &["app"]),
    ("parity-group", &["app"]),
    ("parity-group-toast", &["app"]),
    ("parity-group-notes", &["app"]),
    ("parity-palette", &["app"]),
    ("parity-wall", &["app"]),
    ("parity-empty", &["app"]),
    ("parity-collapsed", &["app"]),
    ("parity-collapsed-ride", &["app"]),
    // (end parity)
];

/// A comma list from the environment; `None` is "everything".
fn wanted(var: &str, known: &[&str]) -> Option<Vec<String>> {
    let list: Vec<String> = std::env::var(var)
        .ok()?
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();
    for item in &list {
        if !known.contains(&item.as_str()) {
            eprintln!(
                "ferrite: {var} names unknown `{item}`; known: {}",
                known.join(",")
            );
        }
    }
    (!list.is_empty()).then_some(list)
}

pub fn capture(output: String) {
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output).expect("create artifact directory");
    let states = wanted(
        "FERRITE_REF_STATES",
        &STATES.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
    );
    let sizes = wanted(
        "FERRITE_REF_SIZES",
        &SIZES.iter().map(|(label, ..)| *label).collect::<Vec<_>>(),
    );
    let platform = gpui::platform::current_platform(true);
    for (label, width, height) in SIZES {
        if sizes
            .as_ref()
            .is_some_and(|sizes| !sizes.iter().any(|s| s == label))
        {
            continue;
        }
        for (state, defaults) in STATES {
            if states
                .as_ref()
                .is_some_and(|states| !states.iter().any(|s| s == state))
            {
                continue;
            }
            // An explicit size list overrides a state's defaults.
            if sizes.is_none() && !defaults.contains(&label) {
                continue;
            }
            render(&output, &platform, state, label, width, height);
        }
    }
}

fn render(
    output: &Path,
    platform: &std::rc::Rc<dyn gpui::Platform>,
    state: &str,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut scene, setup) = build(state, label);
    scene.core.pump();
    let Scene {
        core,
        feeds,
        root,
        hold,
    } = scene;

    let mut cx = HeadlessAppContext::with_platform(
        platform.text_system(),
        std::sync::Arc::new(crate::icons::Assets),
        gpui::platform::current_headless_renderer,
    );
    cx.update(|cx| {
        crate::theme::init_components(cx);
        crate::register_fonts(cx);
        crate::visual::init(cx);
    });
    let mut entity = None;
    let window = cx
        .open_window(
            gpui::size(gpui::px(width), gpui::px(height)),
            |window, cx| {
                let view = cx.new(|cx| CockpitView::new_with_provider(core, Provider::Claude, cx));
                entity = Some(view.clone());
                let content =
                    cx.new(|cx| crate::loops_overlay::CockpitWindow::new(view, window, cx));
                cx.new(|cx| gpui::component::Root::new(content, window, cx))
            },
        )
        .unwrap();
    let view = entity.expect("the window built its view");
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        // A frame first, so anything a setup anchors to has been laid out.
        let _ = window.draw(cx);
        view.update(cx, |view, cx| setup(view, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    // Let toast and popover entrances finish: the toast stack advances its
    // transition one frame at a time, so a settled shot needs frames drawn
    // across the clock, not one jump.
    for _ in 0..8 {
        cx.advance_clock(std::time::Duration::from_millis(150));
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            // There is no platform frame loop here: deliver the frames the
            // last draw asked for, so eased state changes (a disclosure's
            // turn, a fold) land on the clock this loop steps.
            window.simulate_next_frame(cx);
            let _ = window.draw(cx);
        })
        .unwrap();
    }
    // Visuals' pages render in Chromium's own processes, on the wall clock:
    // pump it until they have drawn (nothing without an engine).
    crate::visual::shots::settle(&mut cx, window.into());
    // gpui's `with_animation` runs on the wall clock, not the executor's:
    // a toast's entrance fade is only settled once real time has passed.
    std::thread::sleep(std::time::Duration::from_millis(500));
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        // Two frames: toasts and anchored popovers land on the second.
        let _ = window.draw(cx);
        let _ = window.draw(cx);
        if label == "wide" && state == "edges" {
            let measurements: Vec<_> = ["\tb", "a\tb", "aaaa\tb", "        b"].into_iter().map(|text| {
                let line = window.text_system().shape_line(text.to_owned().into(), gpui::px(crate::theme::FS_UI), &[gpui::TextRun {
                    len:text.len(),font:gpui::font(crate::theme::FONT_CODE),color:gpui::rgb(crate::theme::TEXT).into(),background_color:None,underline:None,strikethrough:None,
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
    // The leak detector runs on drop: release our handle first.
    drop(view);
    // Test-support gpui panics on drop when an entity outlives its app.
    // The image is already saved; report the leak rather than abort the run.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(cx)));
    std::panic::set_hook(hook);
    if let Err(panic) = dropped {
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .unwrap_or("unknown panic");
        let leaked = message.matches("Leaked handle").count();
        eprintln!(
            "ferrite: {state}-{label}: gpui reported {leaked} leaked entity handles at shutdown"
        );
    }
    drop(feeds);
    drop(hold);
    // Only our fresh disposable root; never the operator's store.
    std::fs::remove_dir_all(root).expect("remove disposable reference store");
}

fn build(state: &str, label: &str) -> (Scene, Setup) {
    let none: Setup = Box::new(|_, _, _| {});
    match state {
        state if parity_scenes::handles(state) => parity_scenes::build(state),
        "conversation" => (conversation(label), none),
        "toolgroups" => (tool_groups(label), none),
        "toolgroups-open" => {
            let scene = tool_groups(label);
            let setup: Setup = Box::new(|view, _, cx| {
                use crate::pane::DisclosureId;
                view.panes[0].toggle_tool(&DisclosureId::Group("probe-1".into()));
                view.panes[0].toggle_tool(&DisclosureId::Tool("probe-3".into()));
                if let Some(transcript) = view.panes[0].transcript() {
                    transcript.update(cx, |transcript, cx| {
                        transcript.scroll_to(crate::transcript::ScrollTarget::Top, cx)
                    });
                }
            });
            (scene, setup)
        }
        "nav" => nav(),
        "group4" => group4(),
        "visuals" => (visuals(label), none),
        "visuals-top" => {
            let scene = visuals("top");
            let setup: Setup = Box::new(|view, _, cx| {
                if let Some(transcript) = view.panes[0].transcript() {
                    transcript.update(cx, |transcript, cx| {
                        transcript.scroll_to(crate::transcript::ScrollTarget::Top, cx)
                    });
                }
            });
            (scene, setup)
        }
        "visuals-group" => visuals_group(),
        "group9" => group9(),
        "group12" => group12(false),
        "group12-dragged" => group12(true),
        "subagents" => subagents(),
        "composer" => composer(),
        "menu" => menu(),
        "diff" => {
            let scene = conversation(label);
            let setup: Setup = Box::new(|view, _, _| {
                use crate::pane::DisclosureId;
                // Edits never fold into a group: each Edit shows its hunks
                // once disclosed.
                view.panes[0].toggle_tool(&DisclosureId::Tool("edit-1".into()));
                view.panes[0].toggle_tool(&DisclosureId::Tool("edit-2".into()));
            });
            (scene, setup)
        }
        "subagent" => {
            let (scene, _) = subagents();
            let setup: Setup = Box::new(|view, window, cx| {
                let thread = view.panes[0].thread().expect("a Thread Pane");
                let key = AgentKey::new(Provider::Claude, "fixture", "nav-audit");
                view.select_subject(thread, Subject::Subagent(key), window, cx);
            });
            (scene, setup)
        }
        "modelpicker" => {
            let scene = conversation(label);
            let setup: Setup = Box::new(|view, _, cx| {
                let thread = view.panes[0].thread().expect("a Thread Pane");
                view.open_provider_picker(thread, cx);
            });
            (scene, setup)
        }
        "contextmenu" => {
            let (scene, _) = nav();
            let setup: Setup = Box::new(|view, _, cx| {
                let thread = view.cockpit.roster().focused_thread().expect("focus");
                view.open_context_menu(
                    MenuTarget::Thread(thread),
                    gpui::point(gpui::px(180.), gpui::px(150.)),
                    cx,
                );
            });
            (scene, setup)
        }
        "settings" | "settings-permissions" | "settings-behaviour" | "settings-about" => {
            let scene = conversation(label);
            let page = match state {
                "settings-permissions" => crate::prefs::PageKey::Permissions,
                "settings-behaviour" => crate::prefs::PageKey::Behaviour,
                "settings-about" => crate::prefs::PageKey::About,
                _ => crate::prefs::PageKey::NewThreads,
            };
            let setup: Setup = Box::new(move |view, _, cx| {
                // Seeded, so opening never probes the operator's CLIs.
                view.cli_versions = Some(("2.3.1 (Claude Code)".into(), "codex-cli 0.61.0".into()));
                view.toggle_settings(cx);
                view.settings_page = page;
            });
            (scene, setup)
        }
        "settings-search" => {
            let scene = conversation(label);
            let setup: Setup = Box::new(|view, window, cx| {
                view.cli_versions = Some(("2.3.1 (Claude Code)".into(), "codex-cli 0.61.0".into()));
                view.toggle_settings(cx);
                let search = cx.new(|cx| {
                    gpui::component::input::InputState::new(window, cx)
                        .placeholder("Search")
                        .default_value("codex")
                });
                view.settings_search = Some(search);
            });
            (scene, setup)
        }
        "draft" => {
            let (scene, _) = nav();
            let setup: Setup = Box::new(|view, _, cx| {
                view.open_draft_with_provider(DraftTarget::Main, Provider::Claude, cx);
            });
            (scene, setup)
        }
        "projecteditor" => projecteditor(),
        "notifications" => notifications(),
        "empty-transcript" => {
            let mut scene = Scene::new("empty-transcript");
            let ferrite = scene.project("ferrite");
            let _ = scene.open(Provider::Claude, &ferrite, "");
            (scene, none)
        }
        "starting" => {
            // The prompt is sent, but the Session has not said a word yet.
            let mut scene = Scene::new("starting");
            let ferrite = scene.project("ferrite");
            let (thread, _feed) = scene.open(Provider::Claude, &ferrite, "Starting up");
            scene.core.send(thread, "Summarise the open issues.".into());
            (scene, none)
        }
        "error-turn" => (error_turn(label), none),

        // ---- WP-A scene arms (append above the end line)
        // (end WP-A)

        // ---- WP-B scene arms (append above the end line)
        "prose" => (prose(), none),
        // The reading sizes: the same scenes read at Comfortable and Large.
        "prose-comfortable" => (prose(), reading(ReadingSize::nearest(16))),
        "prose-large" => (prose(), reading(ReadingSize::nearest(18))),
        "formatting-large" => (legacy("formatting").0, reading(ReadingSize::nearest(18))),
        // (end WP-B)

        // ---- WP-C scene arms (append above the end line)
        // The Pane head with a checkout, drift, dirt and a PR whose CI is
        // failing, with its checks card open under the chip.
        "chrome" => {
            let (scene, _) = legacy("live");
            let setup: Setup = Box::new(|view, _, cx| {
                use ferrite_core::workspace::{
                    BranchStatus, Check, CheckState, PrState, PullRequest,
                };
                let thread = view.panes[0].thread().expect("a Thread Pane");
                let run = |name: &str, state, detail: &str, url: bool| Check {
                    name: name.into(),
                    workflow: Some("CI".into()),
                    state,
                    detail: detail.into(),
                    url: url.then(|| "https://example.com/run".into()),
                };
                view.facts.set_branches(vec![(
                    thread,
                    Some(BranchStatus {
                        branch: Some("feat/pane-chrome".into()),
                        upstream: Some("origin/feat/pane-chrome".into()),
                        ahead: 2,
                        behind: 1,
                        dirty: 3,
                        pr: Some(PullRequest {
                            number: 48,
                            state: PrState::Open,
                            draft: false,
                            checks: Some(CheckState::Failing),
                            runs: vec![
                                run(
                                    "test (windows-latest)",
                                    CheckState::Failing,
                                    "failure",
                                    true,
                                ),
                                run(
                                    "test (macos-latest)",
                                    CheckState::Pending,
                                    "in_progress",
                                    true,
                                ),
                                run("fmt", CheckState::Passing, "success", true),
                                run("clippy", CheckState::Skipped, "skipped", false),
                            ],
                        }),
                    }),
                )]);
                view.context_checks = Some(thread);
                cx.notify();
            });
            (scene, setup)
        }
        // The Solo titlebar's `· #48 ●` with its checks card open under it:
        // CI keeps a home when the Pane has no head.
        "checkscard" => {
            let (scene, _) = legacy("live");
            let setup: Setup = Box::new(|view, _, cx| {
                use ferrite_core::workspace::{
                    BranchStatus, Check, CheckState, PrState, PullRequest,
                };
                let thread = view.panes[0].thread().expect("a Thread Pane");
                let run = |name: &str, state, detail: &str| Check {
                    name: name.into(),
                    workflow: Some("CI".into()),
                    state,
                    detail: detail.into(),
                    url: Some("https://example.com/run".into()),
                };
                view.facts.set_branches(vec![(
                    thread,
                    Some(BranchStatus {
                        branch: Some("feat/pane-chrome".into()),
                        upstream: None,
                        ahead: 0,
                        behind: 0,
                        dirty: 0,
                        pr: Some(PullRequest {
                            number: 212,
                            state: PrState::Open,
                            draft: false,
                            checks: Some(CheckState::Failing),
                            runs: vec![
                                run("test (windows-latest)", CheckState::Failing, "failure"),
                                run("test (macos-latest)", CheckState::Pending, "in_progress"),
                                run("docs", CheckState::Pending, "queued"),
                                run("fmt", CheckState::Passing, "success"),
                                run("clippy", CheckState::Skipped, "skipped"),
                                run("release", CheckState::Failing, "cancelled"),
                            ],
                        }),
                    }),
                )]);
                view.context_checks = Some(thread);
                cx.notify();
            });
            (scene, setup)
        }
        // Every Pane closed: the board's start hint.
        "emptyboard" => {
            let mut scene = Scene::new("emptyboard");
            let ferrite = scene.project("ferrite");
            let _ = scene.open(Provider::Claude, &ferrite, "Closed");
            let setup: Setup = Box::new(|view, _, cx| {
                let thread = view.panes[0].thread().expect("a Thread Pane");
                view.close_pane(ferrite_core::roster::PaneIdentity::Thread(thread), cx);
            });
            (scene, setup)
        }
        // (end WP-C)

        // ---- WP-D scene arms (append above the end line)
        "usagecard" => {
            let scene = conversation(label);
            let setup: Setup = Box::new(|view, _, _| {
                let thread = view.panes[0].thread().expect("a Thread Pane");
                view.context_usage = Some(ferrite_core::roster::PaneIdentity::Thread(thread));
            });
            (scene, setup)
        }
        "filescard" => {
            let scene = conversation(label);
            let setup: Setup = Box::new(|view, _, _| {
                let thread = view.panes[0].thread().expect("a Thread Pane");
                view.changed_files_card = Some(thread);
            });
            (scene, setup)
        }
        "preview" => {
            let (scene, compose) = composer();
            let setup: Setup = Box::new(move |view, window, cx| {
                compose(view, window, cx);
                let prompt = view.panes[0].composer.read(cx).prompt();
                let shot = ferrite_core::prompt_files::paths(&prompt, None)
                    .into_iter()
                    .find(|path| path.extension().is_some_and(|ext| ext == "png"));
                if let Some(shot) = shot {
                    let title = shot.file_name().unwrap().to_string_lossy().to_string();
                    view.panes[0].preview.open(shot, title, window, cx);
                }
            });
            (scene, setup)
        }
        "sessioncard" => {
            let scene = conversation(label);
            let setup: Setup = Box::new(|view, _, _| {
                let thread = view.panes[0].thread().expect("a Thread Pane");
                let generation = view.cockpit.thread(thread).expect("open").generation();
                view.session_controls = Some((thread, generation));
            });
            (scene, setup)
        }
        // (end WP-D)

        // ---- WP-E scene arms (append above the end line)
        "projectcreator" => {
            let scene = conversation(label);
            let setup: Setup = Box::new(|view, _, cx| view.open_project_creator(cx));
            (scene, setup)
        }
        // The notifications scene with the panel up: toasts only.
        "toasts" => {
            let (scene, _) = notifications();
            (scene, none)
        }
        // (end WP-E)

        // ---- WP-F scene arms (append above the end line)
        // (end WP-F)

        // ---- WP-G scene arms (append above the end line)
        // The filter row is gone: Project filter, order and new Project are
        // palette commands now. The nav scene with ⌘K asking for them.
        "palettefilter" => {
            let (scene, _) = nav();
            let setup: Setup = Box::new(|view, window, cx| {
                view.open_palette(PaletteScope::All, "project", window, cx);
            });
            (scene, setup)
        }
        // The collapsed rail.
        "navrail" => {
            let (scene, _) = nav();
            let setup: Setup = Box::new(|view, _, cx| {
                view.set_nav_fold(NavFold::Parked, false, cx);
                view.set_nav_collapsed(true, cx);
            });
            (scene, setup)
        }
        // (end WP-G)
        _ => legacy(state),
    }
}

/// The original transcript references: one Solo Thread per state.
fn legacy(state: &str) -> (Scene, Setup) {
    let mut scene = Scene::new(state);
    let checkout = std::env::current_dir().unwrap();
    let (thread, sender) = scene.open(Provider::Claude, &checkout, "");
    let core = &mut scene.core;
    core.send(
        thread,
        "Review the formatting fixture.  Preserve its spacing.".into(),
    );
    sender
        .tool(
            "read",
            "Read",
            serde_json::json!({"file_path": "formatting.md"}),
        )
        .ok("read", "Read the formatting fixture.");
    let source = match state {
        "formatting" => include_str!("../../../docs/research/cli-capture-2026-09-06/model-source/claude-formatting.md").to_string(),
        "edges" => format!("# Heading emphasis\n\n## Second heading\n\n**bold**, *italic*; `one  two` [label](https://example.com/exact?q=a%20b).\n\n9. Nine\n10. Ten with a wrapped continuation of several words\n\nSeparate list:\n\n100. One hundred with a wrapped continuation\n\n> First quote line\n> Second quote line\n>\n> > Nested quote\n\n```text\n    one  two\n\n\tthree   four  \n```\n\n```python\nprint(\"one  two\") # comment\nvalue = 42\n```\n\n| Name | Number | Long token |\n| :--- | ---: | :--- |\n| double  space | 12 | {} |\n| CJK 漢字 | 100 | é NBSP text |\n\n{}", "X".repeat(80), "W".repeat(124)),
        "live" => "I am checking the supplied fixture.\n\n## Streaming heading\n\n- First item\n- Second item\n\n```rust\nlet count = 2;".into(),
        "decision" => "The decision below preserves exact command and answer text.".into(),
        "approval" => "The next command needs your approval.".into(),
        "expanded" => "Tool details remain independently inspectable.".into(),
        _ => "Earlier work remains available after interruption.".into(),
    };
    sender.text(&source);
    for (id, error, result) in [
        ("ok", false, "one  two\nthree   four"),
        ("error", true, "Example failure\nAdditional detail"),
    ] {
        sender
            .tool(
                id,
                "Bash",
                serde_json::json!({"command": "printf 'fixture'"}),
            )
            .done(id, result, error, ToolResult::Opaque);
    }
    match state {
        "live" => {
            sender.ev(SessionEvent::ReasoningSummaryPart {
                item_id: "reasoning".into(),
                summary_index: 0,
                snapshot: true,
                text: "Checking the supplied spacing and tool results".into(),
            });
        }
        "approval" => {
            sender.ev(SessionEvent::DecisionRequested {
                decision: Decision {
                    delivery: Default::default(),
                    kind: DecisionKind::Approval,
                    policy: Default::default(),
                    id: "fixture-approval".into(),
                    tool_use_id: "approval".into(),
                    tool_name: "Bash".into(),
                    description: "Inspect the fixture without modifying files.".into(),
                    suggestions: vec![],
                    input: serde_json::json!({"command":"printf 'one  two\\n'\ncat formatting.md"}),
                },
            });
        }
        "decision" => {
            sender.ev(questions("fixture-question", "question"));
        }
        "interrupted" => {
            sender.ev(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                cost_usd: None,
            });
            core.pump();
            core.send(
                thread,
                "Start a new turn; keep earlier tools inspectable.".into(),
            );
            sender.text("New turn in progress.");
        }
        _ => {
            sender.ev(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            });
        }
    }
    let setup: Setup = if state == "expanded" {
        Box::new(|view, _, _| {
            for id in ["read", "ok"] {
                view.panes[0].toggle_tool(&crate::pane::DisclosureId::Tool(id.into()));
            }
        })
    } else {
        Box::new(|_, _, _| {})
    };
    (scene, setup)
}

fn questions(id: &str, tool_use_id: &str) -> SessionEvent {
    let input = serde_json::json!({"questions":[{"question":"Which details should remain visible?", "multiSelect":true,"options":[{"label":"Keep the existing implementation and its meaningful suffix (Recommended)","description":"Preserve existing controls and all the spacing in their wrapped descriptions."},{"label":"Include detailed output", "description":"Keep disclosure available for inspection."}]}]});
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: DecisionKind::Questions(
                ferrite_core::questions::parse(&input).expect("fixture questions"),
            ),
            policy: Default::default(),
            id: id.into(),
            tool_use_id: tool_use_id.into(),
            tool_name: "AskUserQuestion".into(),
            description: String::new(),
            suggestions: vec![],
            input,
        },
    }
}

const ANSWER_ONE: &str = "Found it. The row height depends on whether the **status line** is \
present, and `Facts::refresh` only fills that line after the first `TextDelta` lands, so every \
Thread grows by one line the moment it starts streaming.\n\n\
The relevant pieces:\n\n\
- `nav_row` sizes itself from `facts.summary`, which stays `None` until the first refresh\n\
- `Facts::refresh` runs on the moment clock, not per frame ([ADR 0006](https://github.com/josephwylie/ferrite/blob/main/docs/adr/0006-retained-transcript-rendering.md))\n\
- parked rows never take this path, which is why only live rows move\n\n\
The fix is to reserve the second line unconditionally and let a placeholder hold the space:\n\n\
```rust\n\
let summary = facts\n    .summary\n    .clone()\n    .unwrap_or_else(|| \"waiting for first output\".into());\n\
row.child(status_line(summary).h(px(theme::ROW_LINE_H)))\n\
```\n\n\
| Row state | Before | After |\n\
| :--- | ---: | ---: |\n\
| idle | 44px | 58px |\n\
| streaming | 58px | 58px |\n\
| parked | 44px | 44px |\n\n\
Want me to apply it and run the nav tests?";

const ANSWER_TWO: &str = "Fixed. Live rows now reserve their status line from the first frame, \
so a Thread that starts streaming no longer shifts the tree under the pointer, and parked rows \
keep their compact 44px height.\n\n\
- `nav.rs`: reserve the status line for open Threads only\n\
- all 38 `nav::` tests pass, including `parked_rows_keep_their_height`\n\n\
I left `facts.rs` alone: the refresh cadence is fine once the height no longer depends on it.";

/// A Solo Thread two turns into a real coding session.
fn conversation(label: &str) -> Scene {
    let mut scene = Scene::new(&format!("conversation-{label}"));
    let ferrite = scene.project("ferrite");
    let (thread, feed) = scene.open(
        Provider::Claude,
        &ferrite,
        "Nav rows jitter on stream start",
    );
    scene.core.send(
        thread,
        "The nav rows jump by a line when a Thread starts streaming. Find out why and propose a fix."
            .into(),
    );
    feed.boot(Provider::Claude, 64_000)
        .tool(
            "grep",
            "Grep",
            serde_json::json!({"pattern": "fn nav_row", "path": "crates/ferrite/src"}),
        )
        .ok(
            "grep",
            "crates/ferrite/src/nav.rs:412:pub(crate) fn nav_row(",
        )
        .tool(
            "read-nav",
            "Read",
            serde_json::json!({"file_path": "crates/ferrite/src/nav.rs"}),
        )
        .ok("read-nav", "1,284 lines")
        .tool(
            "read-facts",
            "Read",
            serde_json::json!({"file_path": "crates/ferrite/src/facts.rs"}),
        )
        .ok("read-facts", "612 lines")
        .text(ANSWER_ONE)
        .end(0.1842);
    scene.core.pump();
    scene
        .core
        .send(thread, "Yes, apply it and run the nav tests.".into());
    feed.edit(
        "edit-1",
        "crates/ferrite/src/nav.rs",
        Hunk {
            old_start: 438,
            old_lines: 3,
            new_start: 438,
            new_lines: 6,
            lines: vec![
                "     let facts = state.facts(thread);".into(),
                "-    if let Some(summary) = facts.summary.clone() {".into(),
                "-        row = row.child(status_line(summary));".into(),
                "+    let summary = facts".into(),
                "+        .summary".into(),
                "+        .clone()".into(),
                "+        .unwrap_or_else(|| \"waiting for first output\".into());".into(),
                "+    row = row.child(status_line(summary).h(px(theme::ROW_LINE_H)));".into(),
            ],
            section: None,
        },
    )
    .bash(
        "test-1",
        "cargo test -p ferrite nav::",
        "running 38 tests\ntest nav::tests::rows_follow_the_roster ... ok\ntest nav::tests::parked_rows_keep_their_height ... FAILED\n\nfailures:\n\n---- nav::tests::parked_rows_keep_their_height stdout ----\nassertion `left == right` failed\n  left: 58.0\n right: 44.0\n\ntest result: FAILED. 37 passed; 1 failed; 0 ignored",
        101,
        18_400,
    )
    .text("One regression: parked rows picked up the reserved line too. Parked rows have no live facts, so the reservation should apply to open Threads only.\n\n")
    .edit(
        "edit-2",
        "crates/ferrite/src/nav.rs",
        Hunk {
            old_start: 438,
            old_lines: 2,
            new_start: 438,
            new_lines: 3,
            lines: vec![
                "     let facts = state.facts(thread);".into(),
                "-    let summary = facts".into(),
                "+    let summary = (!row_state.parked).then(|| facts".into(),
                "+        .summary".into(),
            ],
            section: None,
        },
    )
    .bash(
        "test-2",
        "cargo test -p ferrite nav::",
        "running 38 tests\ntest result: ok. 38 passed; 0 failed; 0 ignored",
        0,
        17_900,
    )
    .text(ANSWER_TWO)
    .end(0.4213);
    scene
}

/// A spread of Threads across three Projects, one a three-member Group,
/// covering every status the nav draws.
fn nav() -> (Scene, Setup) {
    let mut scene = Scene::new("nav");
    let ferrite = scene.project("ferrite");
    let swarmdeck = scene.project("swarmdeck");
    let site = scene.project("flavorlab-site");

    // Parked first, so they are the oldest.
    for (checkout, title) in [
        (&site, "Old spike: wasm preview"),
        (&swarmdeck, "Migrate board storage to SQLite"),
        (&ferrite, "Codex resume handshake"),
    ] {
        let (thread, feed) = scene.open(Provider::Codex, checkout, title);
        scene.core.send(thread, format!("{title}."));
        feed.boot(Provider::Codex, 40_000)
            .text("Done; notes are in the thread.")
            .end(0.05);
        scene.core.pump();
        scene.core.park(thread).expect("park fixture Thread");
    }

    let (running, feed) = scene.open(Provider::Claude, &ferrite, "Retained transcript rendering");
    scene.core.send(
        running,
        "Profile the transcript cache under 24 panes.".into(),
    );
    feed.boot(Provider::Claude, 132_000)
        .text("Profiling the retained path now. The first pass shows layout dominating, not paint:\n\n")
        .bash("prof", "cargo run --release -- --panes 24 --load", "frame p50 6.1ms  p99 11.8ms", 0, 42_000)
        .text("Layout is 71% of the frame. Checking whether the block cache is keyed on width")
        .tool(
            "grep-cache",
            "Grep",
            serde_json::json!({"pattern": "BlockCache", "path": "crates/ferrite/src"}),
        );

    let (geist, geist_feed) = scene.open(Provider::Claude, &ferrite, "Geist type pass");
    scene.core.send(geist, "Swap the UI face to Geist.".into());
    geist_feed
        .boot(Provider::Claude, 58_000)
        .edit(
            "e",
            "crates/ferrite/src/theme.rs",
            Hunk {
                old_start: 12,
                old_lines: 1,
                new_start: 12,
                new_lines: 1,
                lines: vec![
                    "-pub const FONT_PROSE: &str = \"JetBrains Mono\";".into(),
                    "+pub const FONT_PROSE: &str = \"Geist\";".into(),
                ],
                section: None,
            },
        )
        .text("Swapped the prose face; now checking every hard-coded line height against the new metrics");
    let (density, density_feed) = scene.open(Provider::Codex, &ferrite, "Nav density audit");
    scene.core.send(density, "Audit nav row density.".into());
    density_feed
        .boot(Provider::Codex, 21_000)
        .text("I need to regenerate the reference screenshots to compare.\n\n")
        .approval(
            "shot",
            "cargo run -p ferrite --features visual-reference -- --visual-reference /tmp/ref",
        );
    let (harness, harness_feed) = scene.open(Provider::Claude, &ferrite, "Screenshot harness");
    scene.core.send(harness, "Add app-size captures.".into());
    harness_feed
        .boot(Provider::Claude, 77_000)
        .text("Added the `app` size and a state filter.")
        .end(0.2210);
    subagent_crew(&harness_feed, "harness", 5);
    scene.group(&[geist, density, harness], "UI overhaul");

    let (flaky, flaky_feed) = scene.open(Provider::Claude, &ferrite, "Fix flaky pump test");
    scene.core.send(flaky, "Fix the flaky pump test.".into());
    flaky_feed
        .boot(Provider::Claude, 12_000)
        .text("Reproducing under `--test-threads 1`")
        .ev(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Error("API Error: 529 overloaded".into()),
            cost_usd: None,
        });

    let (sync, sync_feed) = scene.open(Provider::Codex, &swarmdeck, "Board card sync");
    scene
        .core
        .send(sync, "Sync board cards with GitHub issues.".into());
    sync_feed
        .boot(Provider::Codex, 9_000)
        .text("Fetching issues")
        .ev(SessionEvent::Closed {
            reason: "codex exited: 401 Unauthorized (run `codex login`)".into(),
        });
    let (notes, notes_feed) = scene.open(Provider::Claude, &swarmdeck, "Release notes 0.9");
    scene
        .core
        .send(notes, "Draft the 0.9 release notes.".into());
    notes_feed
        .boot(Provider::Claude, 31_000)
        .text("Drafted `CHANGELOG.md` for 0.9.")
        .end(0.0930);
    subagent_crew(&notes_feed, "notes", 3);

    let (copy, copy_feed) = scene.open(Provider::Claude, &site, "Checkout copy review");
    scene.core.send(copy, "Review the checkout copy.".into());
    copy_feed
        .boot(Provider::Claude, 18_000)
        .text("Two tones are possible here.")
        .ev(questions("copy-question", "ask"));
    let _ = scene.open(Provider::Codex, &site, "Menu photography brief");

    scene.core.focus_thread(running);
    // Worktree branches and subagent counts beside titles long and short:
    // the title keeps its floor, the branch and then the count give way.
    let setup: Setup = Box::new(move |view, _, cx| {
        view.set_nav_fold(NavFold::Parked, false, cx);
        let branch = |name: &str| {
            Some(ferrite_core::workspace::BranchStatus {
                branch: Some(name.into()),
                ..Default::default()
            })
        };
        view.facts.set_branches(vec![
            (harness, branch("worktree-pay-api-migration-cleanup")),
            (notes, branch("feat/cash-webhook-retries")),
            (sync, branch("fix/sync")),
        ]);
        cx.notify();
    });
    (scene, setup)
}

/// `count` idle subagents under a fixture Main, so its nav row counts them.
fn subagent_crew(feed: &Feed, prefix: &str, count: usize) {
    for n in 0..count {
        let mut info = AgentInfo::new(AgentKey::new(
            Provider::Claude,
            "fixture",
            &format!("{prefix}-{n}"),
        ));
        info.parent = Some(Subject::Main);
        feed.agent(ActivityEvent::Discovered(info));
    }
}

/// `nodes` side by side (or stacked), each an equal share.
fn chain(mut nodes: Vec<Node>, axis: Axis) -> Node {
    if nodes.len() == 1 {
        return nodes.pop().unwrap();
    }
    let share = 1.0 / nodes.len() as f32;
    let head = nodes.remove(0);
    Node::Split {
        axis,
        ratio: share,
        first: Box::new(head),
        second: Box::new(chain(nodes, axis)),
    }
}

fn leaves(threads: &[ThreadId]) -> Vec<Node> {
    threads.iter().map(|thread| Node::Leaf(*thread)).collect()
}

/// Members in a mix of states: streaming, asking, done, failed.
fn members(scene: &mut Scene, checkout: &Path, titles: &[&str]) -> Vec<ThreadId> {
    let mut threads = Vec::new();
    for (at, title) in titles.iter().enumerate() {
        let provider = if at % 3 == 1 {
            Provider::Codex
        } else {
            Provider::Claude
        };
        let (thread, feed) = scene.open(provider, checkout, title);
        scene.core.send(thread, format!("{title}."));
        feed.boot(provider, 20_000 + 9_000 * at as u64);
        match at % 5 {
            0 => {
                feed.text("Wiring the joiner into the canvas path so the atlas stays per-cell; ")
                    .bash(
                        "check",
                        "cargo check --workspace",
                        "Finished dev profile",
                        0,
                        9_100,
                    )
                    .text("the fold keeps the tail following the newest line while")
                    .tool(
                        "test",
                        "Bash",
                        serde_json::json!({"command": "cargo test --workspace"}),
                    );
            }
            1 => {
                feed.text("Closing the superseded issue needs your ruling.\n\n")
                    .approval("close", "gh issue close 212");
            }
            2 => {
                feed.text("Landed the retune behind the theme tokens; the suite is green.")
                    .end(0.31);
            }
            3 => {
                feed.bash(
                    "fail",
                    "cargo test --workspace",
                    "test result: FAILED. 357 passed; 2 failed",
                    101,
                    61_000,
                )
                .text("Two cases regressed after the retune; rerunning the pair with the fold instrumented");
            }
            _ => {
                feed.text("Reading the board recipes side by side before touching anything")
                    .tool(
                        "read",
                        "Read",
                        serde_json::json!({"file_path": "crates/ferrite/src/pane.rs"}),
                    );
            }
        }
        threads.push(thread);
    }
    threads
}

/// Four members on the default grid: 2×2 at L1 at the app size.
fn group4() -> (Scene, Setup) {
    let mut scene = Scene::new("group4");
    let ferrite = scene.project("ferrite");
    let threads = members(
        &mut scene,
        &ferrite,
        &[
            "Perf: layout cache",
            "Close stale issues",
            "Theme retune",
            "Fold regression",
        ],
    );
    let group = scene.group(&threads, "Perf sweep");
    scene.core.enter_group(group).expect("enter fixture Group");
    (scene, Box::new(|_, _, _| {}))
}

/// An agent's visual: the tool call's input, as `show_visual` takes it.
fn visual_input(title: &str, caption: &str, html: &str) -> serde_json::Value {
    serde_json::json!({ "title": title, "caption": caption, "html": html })
}

const STATS_PAGE: &str = include_str!("cockpit/visual_reference/visuals/stats.html");
const MOCKUPS_PAGE: &str = include_str!("cockpit/visual_reference/visuals/mockups.html");
const TREEMAP_PAGE: &str = include_str!("cockpit/visual_reference/visuals/treemap.html");
const VISUAL: &str = ferrite_core::visual::CLAUDE_NAME;

/// A conversation with visuals in every state: one shown (a dashboard),
/// one shown after a call (UI options), one the agent is still drawing
/// (half a treemap), and one that failed.
fn visuals(label: &str) -> Scene {
    let mut scene = Scene::new(&format!("visuals-{label}"));
    let ferrite = scene.project("ferrite");
    let (thread, feed) = scene.open(Provider::Claude, &ferrite, "Token spend, last month");
    scene.core.send(
        thread,
        "Show me token usage and cost for the last 30 days.".into(),
    );
    feed.boot(Provider::Claude, 41_000)
        .bash(
            "usage",
            "ferrite usage --days 30 --json",
            "{\"days\":30,\"cost\":412.80}",
            0,
            900,
        )
        .text("Here is the last 30 days, by day and by model.")
        .tool(
            "v-stats",
            VISUAL,
            visual_input(
                "Token usage, last 30 days",
                "Daily cost with a 7-day average, split by model",
                STATS_PAGE,
            ),
        )
        .ok("v-stats", "Shown to the operator inline.")
        .text("Spend peaks on the days the Group runs overnight; Opus is 71% of it.")
        .end(0.2210);
    scene.core.pump();
    scene.core.send(
        thread,
        "Mock up two options for the settled notice, and show me where the repo's size is.".into(),
    );
    feed.tool(
        "v-mock",
        VISUAL,
        visual_input(
            "Settled notice: now, A and B",
            "Each option with the vertical space it costs",
            MOCKUPS_PAGE,
        ),
    )
    .ok("v-mock", "Shown to the operator inline.")
    .tool("v-bad", VISUAL, visual_input("Repo size by directory", "", "<p>"))
    .done("v-bad", "The visual did not finish rendering within 45s.", true, ToolResult::Opaque)
    .text("That one timed out; drawing it again as a treemap.");
    // Half of the treemap's HTML has streamed in.
    let half = &TREEMAP_PAGE[..TREEMAP_PAGE.len() / 2];
    feed.ev(SessionEvent::ToolDraft {
        id: "v-tree".into(),
        name: VISUAL.into(),
        input: visual_input("Repo size by directory", "Lines of code; click to zoom", half),
    });
    scene
}

/// Four Panes on a board, two with visuals: a visual at a Group Pane's
/// width.
fn visuals_group() -> (Scene, Setup) {
    let mut scene = Scene::new("visuals-group");
    let ferrite = scene.project("ferrite");
    let threads = members(
        &mut scene,
        &ferrite,
        &["Token spend", "Notice options", "Theme retune", "Fold regression"],
    );
    for (index, (title, page)) in [("Token usage", STATS_PAGE), ("Notice options", MOCKUPS_PAGE)]
        .into_iter()
        .enumerate()
    {
        let feed = Feed(scene.feeds.borrow()[index].clone());
        feed.text("Here it is.")
            .tool(&format!("v-{index}"), VISUAL, visual_input(title, "", page))
            .ok(&format!("v-{index}"), "Shown to the operator inline.")
            .end(0.01);
    }
    let group = scene.group(&threads, "Visuals");
    scene.core.enter_group(group).expect("enter fixture Group");
    (scene, Box::new(|_, _, _| {}))
}

/// Nine members on the default grid (3×3), one of them asking a Question.
fn group9() -> (Scene, Setup) {
    let mut scene = Scene::new("group9");
    let ferrite = scene.project("ferrite");
    let titles = [
        "Perf: layout cache",
        "Close stale issues",
        "Theme retune",
        "Fold regression",
        "Board recipes",
        "Pump backpressure",
        "Triage 212",
        "Diff stats badge",
        "Wall census",
    ];
    let threads = members(&mut scene, &ferrite, &titles);
    // "Board recipes" (spawned fifth) stops to ask.
    Feed(scene.feeds.borrow()[4].clone()).ev(questions("board-question", "ask"));
    let group = scene.group(&threads, "Grid of nine");
    scene.core.enter_group(group).expect("enter fixture Group");
    (scene, Box::new(|_, _, _| {}))
}

/// A turn that ends in a provider error after some work.
fn error_turn(label: &str) -> Scene {
    let mut scene = Scene::new(&format!("error-turn-{label}"));
    let ferrite = scene.project("ferrite");
    let (thread, feed) = scene.open(Provider::Claude, &ferrite, "Fix flaky pump test");
    scene.core.send(thread, "Fix the flaky pump test.".into());
    feed.boot(Provider::Claude, 12_000)
        .bash(
            "repro",
            "cargo test -p ferrite-core pump -- --test-threads 1",
            "test result: ok. 14 passed; 0 failed",
            0,
            6_200,
        )
        .text("Reproducing under `--test-threads 1` passes; trying the parallel run next.")
        .ev(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Error("API Error: 529 overloaded".into()),
            cost_usd: None,
        });
    scene
}

/// Runs of routine calls folded into groups: a settled run with failures,
/// a run between two edits, and a run still working, its live call drawn
/// under the summary.
fn tool_groups(label: &str) -> Scene {
    let mut scene = Scene::new(&format!("toolgroups-{label}"));
    let ferrite = scene.project("ferrite");
    let (thread, feed) = scene.open(Provider::Claude, &ferrite, "Reconcile October invoices");
    scene.core.send(
        thread,
        "Reconcile the October supplier invoices against MYOB.".into(),
    );
    let read = |id: &str, path: &str, lines: usize| {
        feed.tool(id, "Read", serde_json::json!({ "file_path": path }))
            .ok(id, &"line\n".repeat(lines));
    };
    feed.boot(Provider::Claude, 64_000);
    feed.bash(
        "probe-1",
        "cd outputs/invoice-reconcile/2026-10-06 && python3 -c 'import json; print(list(json.load(open(\"bills.json\"))[0]))'",
        "['SupplierInvoiceNumber', 'DateDue', 'BalanceDue', 'DateOccurred', 'DisplayId', 'Id']",
        0,
        1_400,
    );
    feed.bash(
        "probe-2",
        "cd outputs/invoice-reconcile/2026-10-06 && python3 match.py --dry-run",
        "",
        0,
        900,
    );
    read("probe-read", "/tmp/saxon.png", 1);
    feed.bash(
        "probe-3",
        "cd outputs/invoice-reconcile/2026-10-06/myob-prep && python3 prep.py --hold",
        "16:HOLD={\n  'a-p01': 'Rivera',\n  'a-p05': 'platform10',\n  'a-p06': 'Tribe Market'\n}\n  # 61 more held",
        0,
        104_000,
    );
    feed.bash(
        "probe-4",
        "cd outputs/invoice-reconcile && grep -n -i '25436\\|adelaide' notes.md",
        "",
        1,
        200,
    );
    feed.tool(
        "probe-5",
        "Grep",
        serde_json::json!({ "pattern": "create_contact", "path": ".claude/skills" }),
    )
    .ok(
        "probe-5",
        ".claude/skills/integrations/myob/scripts/myob.py:316:    emit(args, call(...))",
    );
    feed.bash(
        "probe-6",
        "cd outputs/invoice-reconcile/2026-10-06/myob-prep && python3 accounts.py 6-3001",
        "{'id': '97', 'code': '6-3001', 'name': 'Kitchen Consumables'}\nTraceback (most recent call last):\n  KeyError: 'tax_id'",
        1,
        700,
    );
    feed.bash(
        "probe-7",
        "cd outputs/invoice-reconcile/2026-10-06/myob-prep && python3 prep.py --summary",
        "a-p06 skip Tribe Market (Level Group OMO43465)\n+ 12 bills ready",
        0,
        15_000,
    );
    feed.text("Twelve bills are ready to enter; two need a decision first.\n\n");
    feed.ev(SessionEvent::ContentBoundary);
    feed.edit(
        "fix-1",
        "myob-prep/accounts.py",
        Hunk {
            old_start: 40,
            old_lines: 1,
            new_start: 40,
            new_lines: 1,
            lines: vec![
                "-    tax = row['tax_id']".into(),
                "+    tax = row.get('tax_id', DEFAULT_TAX)".into(),
            ],
            section: None,
        },
    );
    read("check-1", "myob-prep/accounts.py", 120);
    feed.bash(
        "check-2",
        "cd myob-prep && python3 accounts.py 6-3001",
        "{'id': '97', 'code': '6-3001', 'tax_id': '1'}",
        0,
        800,
    );
    feed.text("Accounts resolve now. Entering the drafts.\n\n");
    feed.ev(SessionEvent::ContentBoundary);
    read("live-1", "myob-prep/bills.json", 64);
    feed.tool(
        "live-2",
        "Bash",
        serde_json::json!({ "command": "cd myob-prep && python3 enter.py --draft --all" }),
    );
    scene
}

/// Twelve members on the default grid (4×3, one Level for the board); or,
/// `dragged`, the operator's own row of five over a row of seven, which the
/// board keeps as it was left — at the one Level its smallest cell allows.
fn group12(dragged: bool) -> (Scene, Setup) {
    let mut scene = Scene::new(if dragged {
        "group12-dragged"
    } else {
        "group12"
    });
    let ferrite = scene.project("ferrite");
    let titles = [
        "Perf: layout cache",
        "Close stale issues",
        "Theme retune",
        "Fold regression",
        "Board recipes",
        "Pump backpressure",
        "Triage 212",
        "Diff stats badge",
        "Wall census",
        "Atlas eviction",
        "Seam drag",
        "Resume handshake",
    ];
    let threads = members(&mut scene, &ferrite, &titles);
    let group = scene.group(&threads, "Wall of twelve");
    if dragged {
        let top = chain(leaves(&threads[..5]), Axis::Row);
        let bottom = chain(leaves(&threads[5..]), Axis::Row);
        scene
            .core
            .set_group_layout(
                group,
                Tree {
                    root: Some(chain(vec![top, bottom], Axis::Column)),
                },
            )
            .expect("lay out fixture Group");
    }
    scene.core.enter_group(group).expect("enter fixture Group");
    (scene, Box::new(|_, _, _| {}))
}

/// A Main that delegated to three subagents, each in a different state.
fn subagents() -> (Scene, Setup) {
    let mut scene = Scene::new("subagents");
    let ferrite = scene.project("ferrite");
    let (thread, feed) = scene.open(Provider::Claude, &ferrite, "Audit every surface");
    scene.core.send(
        thread,
        "Audit the nav, the composer and the settings sheet in parallel.".into(),
    );
    feed.boot(Provider::Claude, 91_000)
        .text("Splitting this three ways so each surface gets a focused read.\n\n")
        .tool(
            "task-1",
            "Task",
            serde_json::json!({"description": "Audit the nav", "subagent_type": "Explore"}),
        )
        .tool(
            "task-2",
            "Task",
            serde_json::json!({"description": "Audit the composer", "subagent_type": "Explore"}),
        )
        .tool(
            "task-3",
            "Task",
            serde_json::json!({"description": "Audit settings", "subagent_type": "Explore"}),
        );
    for (name, description, status, body) in [
        (
            "nav-audit",
            "Audit the nav",
            AgentStatus::Working,
            "Reading `nav.rs` row recipes. Row heights vary between 44px and 58px depending on",
        ),
        (
            "composer-audit",
            "Audit the composer",
            AgentStatus::Idle,
            "The composer is consistent: one caret, one tray, queued prompts stack newest first.\n\n",
        ),
        (
            "settings-audit",
            "Audit settings",
            AgentStatus::Failed,
            "Could not open `prefs.rs`: permission denied.\n\n",
        ),
    ] {
        let key = AgentKey::new(Provider::Claude, "fixture", name);
        let mut info = AgentInfo::new(key.clone());
        info.name = Some(name.into());
        info.description = Some(description.into());
        info.kind = Some("Explore".into());
        info.parent = Some(Subject::Main);
        info.coverage = TranscriptCoverage::Live;
        feed.agent(ActivityEvent::Discovered(info))
            .agent(ActivityEvent::Content {
                key: key.clone(),
                id: Some(format!("{name}-read")),
                event: ExecutionEvent::ToolStarted {
                    id: format!("{name}-read"),
                    name: "Read".into(),
                    input: serde_json::json!({"file_path": "crates/ferrite/src/nav.rs"}),
                },
            })
            .agent(ActivityEvent::Content {
                key: key.clone(),
                id: Some(format!("{name}-text")),
                event: ExecutionEvent::Text { text: body.into() },
            })
            .agent(ActivityEvent::Status { key, state: status });
    }
    (scene, Box::new(|_, _, _| {}))
}

/// Mid-turn with two prompts held for the next turn, a third being typed,
/// and two files attached to it.
fn composer() -> (Scene, Setup) {
    let mut scene = Scene::new("composer");
    let ferrite = scene.project("ferrite");
    let (thread, feed) = scene.open(Provider::Claude, &ferrite, "Composer queue polish");
    scene
        .core
        .send(thread, "Tighten the queued prompt rows.".into());
    feed.boot(Provider::Claude, 104_000)
        .tool(
            "read",
            "Read",
            serde_json::json!({"file_path": "crates/ferrite/src/pane.rs"}),
        )
        .ok("read", "6,569 lines")
        .text("The queued rows sit at `CELL_HEADER_H`; checking how the stack clips when three or more are held");
    scene.core.pump();
    scene.core.queue(thread, "Then run the pane tests.".into());
    scene.core.queue(
        thread,
        "Also check the take-back key still only applies to the top row.".into(),
    );
    let notes = scene.root.join("queue-notes.md");
    std::fs::write(&notes, "# Queue notes\n").expect("write fixture attachment");
    let shot = scene.root.join("queued-rows.png");
    let _ = std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/research/ferrite-transcript-implementation-2026-09-06/wide.png"),
        &shot,
    );
    let setup: Setup = Box::new(move |view, _, cx| {
        let files: Vec<PathBuf> = [shot, notes].into_iter().filter(|p| p.exists()).collect();
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.set(
                "When that lands, compare the stack against the design comp\nand note any row that clips."
                    .into(),
                cx,
            );
            composer.add_files(&files, cx);
        });
    });
    (scene, setup)
}

/// The `/` popover over a Thread that announced its commands.
fn menu() -> (Scene, Setup) {
    let mut scene = conversation("menu");
    let feed = Feed(scene.feeds.borrow()[0].clone());
    feed.ev(SessionEvent::Commands {
        commands: [
            ("code-review", "Review the changes since a fixed point"),
            ("commit", "Commit staged changes with a generated message"),
            ("compact", "Summarise the conversation to free context"),
            ("context", "Show what is using the context window"),
            ("init", "Initialize a CLAUDE.md for this repository"),
            (
                "simplify",
                "Review changed code for reuse and simplification",
            ),
        ]
        .into_iter()
        .map(|(name, description)| SessionCommand {
            name: name.into(),
            description: description.into(),
            path: None,
        })
        .collect(),
    });
    scene.core.pump();
    let setup: Setup = Box::new(|view, _, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("/".into(), cx));
        view.sync_menu(cx);
    });
    (scene, setup)
}

/// The Project card on a two-directory Project.
fn projecteditor() -> (Scene, Setup) {
    let mut scene = conversation("projecteditor");
    let extra = scene.project("ferrite-docs");
    let project = scene
        .core
        .registry()
        .projects()
        .first()
        .map(|project| project.id)
        .expect("a fixture Project");
    scene
        .core
        .add_project_directories(project, &[extra])
        .expect("add fixture directory");
    let setup: Setup = Box::new(move |view, _, cx| view.open_project_editor(project, cx));
    (scene, setup)
}

/// Background Threads that finished while the operator was elsewhere: the
/// bell's panel down over its Notices, with their toasts.
fn notifications() -> (Scene, Setup) {
    let mut scene = conversation("notifications");
    let ferrite = scene.project("ferrite");
    let swarmdeck = scene.project("swarmdeck");
    let focused = scene.core.roster().focused_thread().expect("focus");
    let mut others = Vec::new();
    for (checkout, title, outcome) in [
        (&ferrite, "Theme retune", TurnOutcome::Completed),
        (
            &swarmdeck,
            "Board card sync",
            TurnOutcome::Error("API Error: 529 overloaded".into()),
        ),
        (&ferrite, "Release notes 0.9", TurnOutcome::Completed),
    ] {
        let (thread, feed) = scene.open(Provider::Claude, checkout, title);
        scene.core.send(thread, format!("{title}."));
        feed.boot(Provider::Claude, 30_000)
            .text("Working through it.");
        others.push((feed, outcome));
    }
    let (asking, ask_feed) = scene.open(Provider::Codex, &ferrite, "Close stale issues");
    scene.core.send(asking, "Close stale issues.".into());
    ask_feed.boot(Provider::Codex, 12_000);
    scene.core.pump();
    scene.core.focus_thread(focused);
    for (feed, outcome) in &others {
        feed.text(" Done.").ev(SessionEvent::TurnEnded {
            outcome: outcome.clone(),
            cost_usd: Some(0.12),
        });
    }
    ask_feed.approval("close", "gh issue close 212");
    let setup: Setup = Box::new(|view, window, cx| {
        view.toggle_notifications(&super::ToggleNotifications, window, cx);
    });
    (scene, setup)
}

// ---- WP-A scene builders (append above the end line)
// (end WP-A)

// ---- WP-B scene builders (append above the end line)

/// A setup that reads the Solo Pane at `size`.
fn reading(size: ReadingSize) -> Setup {
    Box::new(move |view, _, cx| {
        view.prefs.settings.reading_size = size;
        cx.notify();
    })
}

/// Markdown the other states do not reach: file chips (with a line, an
/// image, a long name), an html fence with Preview, a highlighted Rust fence,
/// headings mid-answer and a table with aligned columns.
fn prose() -> Scene {
    let mut scene = Scene::new("prose");
    let checkout = std::env::current_dir().unwrap();
    let (thread, sender) = scene.open(Provider::Claude, &checkout, "");
    scene
        .core
        .send(thread, "Where does the answer layout live?".into());
    sender.text(
        "The answer row is built in [transcript.rs](crates/ferrite/src/transcript.rs:405), and its \
         Markdown in [rich.rs](crates/ferrite/src/rich.rs). The icon is \
         [app-icon.png](crates/ferrite/assets/app-icon.png); the long one is \
         [2026-09-15-ui-polish-implementation/README.md](docs/audits/2026-09-15-ui-polish-implementation/README.md).\n\n\
         ## What changed\n\n\
         Prose now reads at **14/22** with `Geist`; code keeps `Geist Mono`.\n\n\
         ### Details\n\n\
         ```rust\n\
         // Headings scale with the reading size.\n\
         pub fn heading_scale(level: u8) -> f32 {\n    match level {\n        1 => 18. / 14.,\n        _ => 1.,\n    }\n}\n\
         let size = Pixels::from(base * heading_scale(2));\n\
         ```\n\n\
         ```html\n<p>Hello <b>preview</b></p>\n```\n\n\
         | Size | Prose | Line |\n| :--- | ---: | ---: |\n| Standard | 14 | 22 |\n| Large | 18 | 28 |\n\n\
         ---\n\n\
         #### A quiet heading\n\n\
         That is all.",
    );
    sender.ev(SessionEvent::TurnEnded {
        outcome: TurnOutcome::Completed,
        cost_usd: None,
    });
    scene
}
// (end WP-B)

// ---- WP-C scene builders (append above the end line)
// (end WP-C)

// ---- WP-D scene builders (append above the end line)
// (end WP-D)

// ---- WP-E scene builders (append above the end line)
// (end WP-E)

// ---- WP-F scene builders (append above the end line)
// (end WP-F)

// ---- WP-G scene builders (append above the end line)
// (end WP-G)

/// The loops overlay's pixel parity (`loops_overlay`): one scene — a
/// working Thread focused in an active window, its working line's star and
/// shimmer, the nav's braille spinner, the Composer's caret blinking over a
/// character of typed text — drawn with the overlay and without it, at the
/// same instants every 20ms across more than a blink, must give the same
/// framebuffer; then again with the palette open over it, its caret
/// blinking on the float. Writes `loops-parity.json` (the palette pass under
/// `palette`) and, for any instant that differs, both shots.
pub fn loops_parity(output: String) {
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output).expect("create artifact directory");
    let platform = gpui::platform::current_platform(true);
    let steps = std::env::var("FERRITE_PARITY_STEPS")
        .ok()
        .and_then(|steps| steps.parse().ok())
        .unwrap_or(65u64);
    let instants: Vec<u64> = (0..=steps).map(|step| step * 20).collect();
    let mut report = loops_parity_pass(&platform, false, &instants, &output);
    report["palette"] = loops_parity_pass(&platform, true, &instants, &output);
    std::fs::write(
        output.join("loops-parity.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

/// One parity pass: the scene, the palette open over it or not, drawn with
/// and without the overlay; answers the pass's report and saves the shots
/// of any instant that differs.
fn loops_parity_pass(
    platform: &std::rc::Rc<dyn gpui::Platform>,
    palette: bool,
    instants: &[u64],
    output: &std::path::Path,
) -> serde_json::Value {
    let mut on = Vec::new();
    let on_hosted = loops_frames(platform, true, palette, instants, |cx, window| {
        on.push(cx.capture_screenshot(window).unwrap())
    });
    let mut off = Vec::new();
    let off_hosted = loops_frames(platform, false, palette, instants, |cx, window| {
        off.push(cx.capture_screenshot(window).unwrap())
    });
    let pass = if palette { "palette-" } else { "" };
    // The premise: the loops moved — the instants show different pictures.
    let distinct = {
        let mut frames: Vec<&[u8]> = on.iter().map(|frame| frame.as_raw().as_slice()).collect();
        frames.sort_unstable();
        frames.dedup();
        frames.len()
    };
    let mut differing = Vec::new();
    for ((at, with), without) in instants.iter().zip(&on).zip(&off) {
        if with.as_raw() == without.as_raw() {
            continue;
        }
        let mut pixels = 0usize;
        let mut worst = 0u8;
        let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
        for (x, y, a) in with.enumerate_pixels() {
            let b = without.get_pixel(x, y);
            let delta =
                a.0.iter()
                    .zip(b.0.iter())
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0);
            if delta > 0 {
                pixels += 1;
                worst = worst.max(delta);
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
        }
        with.save(output.join(format!("loops-{pass}{at:04}ms-overlay.png")))
            .unwrap();
        without
            .save(output.join(format!("loops-{pass}{at:04}ms-in-place.png")))
            .unwrap();
        differing.push(serde_json::json!({
            "ms": at, "pixels": pixels, "max_channel_delta": worst,
            "bounds": [left, top, right, bottom],
        }));
    }
    let report = serde_json::json!({
        "instants": instants.len(),
        "distinct_frames": distinct,
        "overlay_hosted_marks": on_hosted,
        "in_place_hosted_marks": off_hosted,
        "differing": differing,
    });
    eprintln!(
        "LOOPS_PARITY palette={palette} instants={} distinct_frames={distinct} differing={} overlay_marks={on_hosted:?} in_place_marks={off_hosted:?}",
        instants.len(),
        report["differing"].as_array().map_or(0, Vec::len),
    );
    report
}

/// The parity scene drawn at each of `instants` (ms from its first frame),
/// with or without the overlay, the palette open over it or not, each frame
/// handed to `shot`; answers how many loops the overlay drew.
fn loops_frames(
    platform: &std::rc::Rc<dyn gpui::Platform>,
    overlay: bool,
    palette: bool,
    instants: &[u64],
    mut shot: impl FnMut(&mut HeadlessAppContext, gpui::AnyWindowHandle),
) -> Vec<usize> {
    // One frozen wall clock and one store path for both passes: the clock
    // text and the checkout's path read the same in each.
    let clock = crate::demo::parity::Fixture::install(
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_791_400_000),
    );
    let mut scene = Scene::new("loops-parity");
    let ferrite = scene.project("ferrite");
    let (thread, feed) = scene.open(Provider::Claude, &ferrite, "Caret parity");
    scene
        .core
        .send(thread, "Find out why the caret blinks.".into());
    feed.boot(Provider::Claude, 64_000)
        .ev(SessionEvent::ReasoningSummaryDelta {
            text: "**Checking the blink**".into(),
            summary_index: 0,
        });
    let (other, other_feed) = scene.open(Provider::Claude, &ferrite, "Spinner in the nav");
    scene.core.send(other, "Keep going.".into());
    other_feed
        .boot(Provider::Claude, 32_000)
        .text("Working on it.");
    scene.core.pump();
    let Scene {
        core,
        feeds,
        root,
        hold,
    } = scene;
    let mut cx = HeadlessAppContext::with_platform(
        platform.text_system(),
        std::sync::Arc::new(crate::icons::Assets),
        gpui::platform::current_headless_renderer,
    );
    cx.update(|cx| {
        crate::theme::init_components(cx);
        crate::register_fonts(cx);
    });
    let mut entity = None;
    let window = cx
        .open_window(gpui::size(gpui::px(1200.), gpui::px(750.)), |window, cx| {
            let view = cx.new(|cx| CockpitView::new_with_provider(core, Provider::Claude, cx));
            entity = Some(view.clone());
            let content = cx.new(|cx| {
                crate::loops_overlay::CockpitWindow::with_overlay(view, overlay, window, cx)
            });
            cx.new(|cx| gpui::component::Root::new(content, window, cx))
        })
        .unwrap();
    let view = entity.expect("the window built its view");
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        let _ = window.draw(cx);
        window.activate_window();
        view.update(cx, |view, cx| {
            let index = view
                .panes
                .iter()
                .position(|pane| pane.thread() == Some(thread))
                .expect("the working Thread's Pane");
            view.focus_pane(index);
            let composer = view.panes[index].composer.clone();
            composer.update(cx, |composer, cx| {
                composer.insert("blink over this text", cx);
            });
            window.focus(&gpui::Focusable::focus_handle(composer.read(cx), cx), cx);
            if palette {
                view.open_palette(crate::palette::PaletteScope::All, "caret", window, cx);
            }
            cx.notify();
        });
    })
    .unwrap();
    // Wall-clock entrances (toasts, popovers) settle before the shots.
    for _ in 0..4 {
        cx.advance_clock(std::time::Duration::from_millis(150));
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.simulate_next_frame(cx);
            let _ = window.draw(cx);
        })
        .unwrap();
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    // The caret one character back from the end: it stands on a glyph.
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_action(Box::new(crate::composer::Left), cx);
        let _ = window.draw(cx);
    })
    .unwrap();
    let mut hosted = Vec::new();
    let mut at = 0;
    for instant in instants {
        cx.advance_clock(std::time::Duration::from_millis(instant - at));
        at = *instant;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            let _ = window.draw(cx);
            hosted.push(crate::loops_overlay::hosted_marks(window));
        })
        .unwrap();
        shot(&mut cx, window.into());
    }
    drop(view);
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(cx)));
    std::panic::set_hook(hook);
    drop(feeds);
    drop(hold);
    std::fs::remove_dir_all(root).expect("remove disposable reference store");
    drop(clock);
    hosted.dedup();
    hosted
}
