mod attention_alerts;
mod boards;
mod checks;
mod completion_checks;
mod composer_chips;
mod composer_controls;
mod composer_menus;
mod core_transcript_parity;
mod decision_cards;
mod decisions_parity;
mod diagnostics;
mod find;
mod floats_parity;
mod frame_parity;
mod fullscreen;
mod image_viewer;
mod interaction_regressions;
mod key_shortcuts;
mod layout_polish;
mod leaving;
mod link_clicks;
mod loops_overlay;
mod machines;
mod nav_list;
mod nav_parity;
mod panel;
mod panel_preview;
mod parking;
mod pr;
mod pr_tab;
mod project_picker;
#[cfg(unix)]
mod projects;
mod provider_controls;
mod provider_forms;
mod provider_navigation;
mod provider_picker;
mod queued_prompts;
mod reader_motion;
mod render_performance;
mod retention;
mod review;
mod session_import;
mod settings_window;
mod stab;
mod stale_chrome;
mod startup;
mod subagents;
mod thread_groups;
mod transcript_pointer;
mod transcript_rows;
mod ui_a;
mod ui_b;
mod ui_c;
mod ui_d;
mod ui_e;
mod ui_f;
mod ui_g;
mod updating;
mod user_commands;
mod visuals;
mod wall;
mod workspace_binding;

use super::*;

use std::cell::RefCell;

use std::rc::Rc;

use std::sync::mpsc::{self, Receiver, Sender};

use ferrite_core::cockpit::Spawner;

use ferrite_core::providers::Session;

use ferrite_core::store::Store;

use ferrite_core::transcript::Body;

use ferrite_core::workspace::WorkspaceBinding;

use ferrite_core::{Decision, SessionEvent};

use gpui::{KeyBinding, TestAppContext};

struct Scripted {
    tx: Sender<SessionEvent>,
    rx: Receiver<SessionEvent>,
    interrupts: Rc<RefCell<usize>>,
    stopped: Rc<RefCell<Vec<ferrite_core::activity::AgentKey>>>,
    fail_send: Rc<RefCell<bool>>,
    sent: Rc<RefCell<Vec<String>>>,
    answered: Rc<RefCell<Vec<(String, DecisionAnswer)>>>,
    controls: Rc<RefCell<Vec<ferrite_core::SessionControl>>>,
    native_controls: Rc<RefCell<bool>>,
    file_searches: Rc<RefCell<Vec<(String, ferrite_core::providers::FileSearchReply)>>>,
    native_files: Rc<RefCell<bool>>,
    effort_answers: EffortAnswers,
}

/// While `Some`, the provider answers each effort change when the test
/// sends on its answer, not at once.
type EffortAnswers = Rc<RefCell<Option<Vec<mpsc::SyncSender<std::io::Result<()>>>>>>;

impl Session for Scripted {
    fn search_files(
        &mut self,
        query: &str,
        reply: ferrite_core::providers::FileSearchReply,
    ) -> std::io::Result<()> {
        if !*self.native_files.borrow() {
            return Err(std::io::ErrorKind::Unsupported.into());
        }
        self.file_searches.borrow_mut().push((query.into(), reply));
        Ok(())
    }

    fn permission_modes(&self) -> Vec<ferrite_core::PermissionModeChoice> {
        if *self.native_controls.borrow() {
            vec![ferrite_core::PermissionModeChoice {
                value: "native-mode".into(),
                label: "Ask for changes".into(),
            }]
        } else {
            vec![]
        }
    }
    fn supports_control(&self, _: ferrite_core::ControlKind) -> bool {
        *self.native_controls.borrow()
    }
    fn control(&mut self, action: ferrite_core::SessionControl) -> std::io::Result<()> {
        self.controls.borrow_mut().push(action);
        Ok(())
    }
    fn enqueue(&mut self, id: &str, text: &str) -> std::io::Result<()> {
        self.tx
            .send(SessionEvent::Queue(ferrite_core::QueueEvent::Accepted(
                ferrite_core::QueuedPrompt {
                    id: id.into(),
                    client_id: id.into(),
                    text: text.into(),
                },
            )))
            .unwrap();
        Ok(())
    }
    fn cancel_queued(&mut self, id: &str) -> std::io::Result<()> {
        self.tx
            .send(SessionEvent::Queue(ferrite_core::QueueEvent::Cancelled {
                id: id.into(),
                cancelled: true,
                error: None,
            }))
            .unwrap();
        Ok(())
    }

    fn set_effort(
        &mut self,
        _effort: Option<&str>,
    ) -> std::io::Result<ferrite_core::providers::SettingAck> {
        match self.effort_answers.borrow_mut().as_mut() {
            Some(answers) => {
                let (answer, ack) = mpsc::sync_channel(1);
                answers.push(answer);
                Ok(ack)
            }
            None => Ok(ferrite_core::providers::applied()),
        }
    }
    fn events(&self) -> &Receiver<SessionEvent> {
        &self.rx
    }
    fn send(&mut self, text: &str) -> std::io::Result<()> {
        if *self.fail_send.borrow() {
            return Err(std::io::Error::other("stub refused first prompt"));
        }
        self.sent.borrow_mut().push(text.to_string());
        Ok(())
    }
    fn interrupt(&mut self) -> std::io::Result<()> {
        *self.interrupts.borrow_mut() += 1;
        Ok(())
    }
    fn stop_subagent(&mut self, key: &ferrite_core::activity::AgentKey) -> std::io::Result<()> {
        self.stopped.borrow_mut().push(key.clone());
        Ok(())
    }
    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> std::io::Result<()> {
        self.answered.borrow_mut().push((id.to_string(), answer));
        Ok(())
    }
}

#[derive(Clone, Default)]
struct Fake {
    interrupts: Rc<RefCell<usize>>,
    stopped: Rc<RefCell<Vec<ferrite_core::activity::AgentKey>>>,
    model_discovery: Rc<RefCell<Option<Receiver<(Provider, Vec<ferrite_core::ModelInfo>)>>>>,
    session_discovery:
        Rc<RefCell<Option<Receiver<std::io::Result<Vec<ferrite_core::import::Candidate>>>>>>,
    command_discovery: Rc<RefCell<Option<ferrite_core::providers::commands::Discovery>>>,
    command_requests: Rc<RefCell<Vec<(Provider, std::path::PathBuf)>>>,
    streams: Rc<RefCell<Vec<Sender<SessionEvent>>>>,
    /// Every spawn's choice, in call order — what the provider-picker
    /// tests read back (#25).
    spawned: Rc<RefCell<Vec<ProviderChoice>>>,
    /// While set, spawn refuses — how a test fails a bootstrap (#29).
    fail: Rc<RefCell<bool>>,
    fail_send: Rc<RefCell<bool>>,
    sent: Rc<RefCell<Vec<String>>>,
    /// Every Decision answer that went out, with the Decision's id.
    answered: Rc<RefCell<Vec<(String, DecisionAnswer)>>>,
    controls: Rc<RefCell<Vec<ferrite_core::SessionControl>>>,
    native_controls: Rc<RefCell<bool>>,
    file_searches: Rc<RefCell<Vec<(String, ferrite_core::providers::FileSearchReply)>>>,
    native_files: Rc<RefCell<bool>>,
    effort_answers: EffortAnswers,
}

impl Spawner for Fake {
    fn discover_sessions(
        &mut self,
        _: Vec<(Provider, std::path::PathBuf)>,
        _: usize,
    ) -> Option<Receiver<std::io::Result<Vec<ferrite_core::import::Candidate>>>> {
        self.session_discovery.borrow_mut().take()
    }
    fn discover_commands(
        &mut self,
        provider: Provider,
        cwd: &std::path::Path,
    ) -> Option<ferrite_core::providers::commands::Discovery> {
        self.command_requests
            .borrow_mut()
            .push((provider, cwd.to_path_buf()));
        self.command_discovery.borrow_mut().take()
    }

    fn discover_models(&mut self) -> Option<Receiver<(Provider, Vec<ferrite_core::ModelInfo>)>> {
        self.model_discovery.borrow_mut().take()
    }

    fn spawn(
        &mut self,
        request: ferrite_core::cockpit::SpawnRequest,
    ) -> std::io::Result<Box<dyn Session>> {
        if *self.fail.borrow() {
            return Err(std::io::Error::other("stub refused to spawn"));
        }
        let (tx, rx) = mpsc::channel();
        self.streams.borrow_mut().push(tx.clone());
        self.spawned.borrow_mut().push(ProviderChoice {
            provider: request.provider,
            model: request.model.map(|model| model.to_string()),
        });
        Ok(Box::new(Scripted {
            tx,
            rx,
            interrupts: self.interrupts.clone(),
            stopped: self.stopped.clone(),
            fail_send: self.fail_send.clone(),
            sent: self.sent.clone(),
            answered: self.answered.clone(),
            controls: self.controls.clone(),
            native_controls: self.native_controls.clone(),
            file_searches: self.file_searches.clone(),
            native_files: self.native_files.clone(),
            effort_answers: self.effort_answers.clone(),
        }))
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-view-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Put every open Thread into one Group. A multi-Pane view **is** a Group
/// (#28), so any test that wants more than one Pane on screen has to say
/// which Group they share — `View::Solo` shows exactly the focused Pane.
fn group_all(cockpit: &mut Cockpit) -> GroupId {
    let threads = cockpit.threads();
    let group = cockpit
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    for thread in &threads[2..] {
        cockpit
            .apply_group(GroupChange::Join {
                thread: *thread,
                group,
                index: None,
            })
            .unwrap();
    }
    group
}

/// Hold this test's wall clock still, for a test that counts renders. The
/// executor's clock is stepped, but a nav row's age, a notice's and the
/// bottom bar's minute turn on the wall clock, at the machine's pace: one
/// turning inside a counted window (a loaded machine stretches the window
/// into real seconds) redraws until the real clock catches the stepped
/// one. Install before the Cockpit, so every stamp reads this time.
fn hold_wall_clock() -> ferrite_core::clock::ThreadFixture {
    // Mid-minute, so nothing that reads it sits on a turn.
    let at = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_791_399_990);
    ferrite_core::clock::ThreadFixture::install(at.into())
}

fn cockpit(name: &str, panes: usize) -> (Cockpit, Fake) {
    let fake = Fake::default();
    let store = Store::open(scratch(name)).unwrap();
    let mut cockpit = Cockpit::new(store, Box::new(fake.clone()));
    for _ in 0..panes {
        cockpit
            .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
            .unwrap();
    }
    (cockpit, fake)
}

/// The Attention list's sections, as `attention_list` builds them.
fn listed(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<(nav::ListSection, Vec<ThreadId>)> {
    view.read_with(cx, |view, _| {
        view.attention_list(&view.nav_state())
            .sections
            .into_iter()
            .map(|block| {
                let rows = block.rows.iter().map(|row| row.thread).collect();
                (block.section, rows)
            })
            .collect()
    })
}

// Match main: Root mounts the toolkit selection layer, menus and dialogs.
// The window opens focused, as a launched one does; what is on screen
// is seen from its first frame.
fn add_cockpit_window(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut Context<CockpitView>) -> CockpitView,
) -> (Entity<CockpitView>, &mut gpui::VisualTestContext) {
    let (view, cx) = add_unfocused_cockpit_window(cx, build);
    cx.update(|window, _| window.activate_window());
    (view, cx)
}

/// A Cockpit window that opens without the operator's focus.
fn add_unfocused_cockpit_window(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut Context<CockpitView>) -> CockpitView,
) -> (Entity<CockpitView>, &mut gpui::VisualTestContext) {
    let (root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| build(window, cx));
        gpui::component::Root::new(
            cx.new(|cx| crate::cockpit_window::CockpitWindow::new(view, window, cx)),
            window,
            cx,
        )
    });
    let view = root.read_with(cx, |root, cx| {
        crate::cockpit_window::testing::cockpit(root.view(), cx)
    });
    (view, cx)
}

fn debug_bounds(
    cx: &mut gpui::VisualTestContext,
    id: String,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(id.into_boxed_str()))
}

fn bounds(cx: &mut gpui::VisualTestContext, id: String) -> gpui::Bounds<gpui::Pixels> {
    debug_bounds(cx, id.clone()).unwrap_or_else(|| panic!("missing {id}"))
}

/// Let the pump's timer fire: the test clock does not move on its own.
/// The operator's cmd-B override of the narrow-window rail (rule
/// 2.7.7): the full column stays open however narrow the window gets,
/// so a test can stand a narrow board beside it.
fn hold_nav_open(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, _| view.nav_forced_open = true);
}

/// Turn the panel off, for a test whose geometry is the board without
/// it: the panel shows at any width (SPEC §5.6), so a narrow window's
/// board, nav and Levels are otherwise the panel's business. In
/// memory only: the suite's windows share one settings file.
fn hide_panel(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        view.prefs.settings.panel_shown = false;
        cx.notify();
    });
}

fn tick(cx: &mut gpui::VisualTestContext) {
    cx.executor()
        .advance_clock(Duration::from_millis(PUMP_MS * 4));
    cx.run_until_parked();
}

/// Move the executor's clock `by` in one step, as a machine waking from
/// sleep does, then pump: a timer due in between fires once, at the new
/// time. `advance_clock` would instead run the pump every `PUMP_MS` of
/// the way, seconds of real time for a 30 s wait that asserts nothing.
fn leap(cx: &mut gpui::VisualTestContext, by: Duration) {
    cx.executor()
        .dispatcher()
        .as_test()
        .expect("a test executor")
        .scheduler()
        .clock()
        .advance(by);
    tick(cx);
}

/// Pump until `done`, waiting between looks for off-thread work (a git
/// read, a command, a Server's message) to land (`ferrite_core::doorbell`),
/// not for wall-clock time. Each look leaps the clock by `ENDED_POLL`, the
/// longest the window waits before it looks for what landed.
fn until(
    cx: &mut gpui::VisualTestContext,
    mut done: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) {
    ferrite_core::doorbell::until("never happened", || {
        leap(cx, super::user_commands::ENDED_POLL);
        done(cx)
    });
}

/// Pump past the alerts' delay: whatever still stands has rung (ADR
/// 0017).
fn ring(cx: &mut gpui::VisualTestContext) {
    cx.executor()
        .advance_clock(crate::notifications::ALERT_DELAY);
    tick(cx);
}

fn decision(id: &str) -> SessionEvent {
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: id.into(),
            tool_use_id: "toolu_1".into(),
            tool_name: "Write".into(),
            description: "ferrite-perm.txt".into(),
            input: serde_json::Value::Null,
            suggestions: vec![],
        },
    }
}

/// A PR whose checks are mixed — one Actions job failing, one passing,
/// and a posted commit status beside them.
fn pr_with_checks() -> ferrite_core::workspace::PullRequest {
    use ferrite_core::workspace::{Check, CheckState, PrState, PullRequest};
    let run = |name: &str, workflow: Option<&str>, state, detail: &str, url: Option<&str>| Check {
        name: name.to_string(),
        workflow: workflow.map(str::to_string),
        state,
        detail: detail.to_string(),
        url: url.map(str::to_string),
    };
    PullRequest {
        number: 48,
        state: PrState::Open,
        draft: false,
        url: Some("https://github.com/o/r/pull/48".into()),
        checks: Some(CheckState::Failing),
        runs: vec![
            run(
                "test (windows-latest)",
                Some("CI"),
                CheckState::Failing,
                "failure",
                Some("https://github.com/o/r/actions/runs/1"),
            ),
            run("fmt", Some("CI"), CheckState::Passing, "success", None),
            run(
                "codecov/patch",
                None,
                CheckState::Pending,
                "pending",
                Some("https://codecov.io/x"),
            ),
        ],
    }
}

fn question(id: &str) -> SessionEvent {
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: ferrite_core::DecisionKind::Questions(vec![ferrite_core::questions::Question {
                id: None,
                question: "Which approach?".into(),
                header: "Approach".into(),
                multi_select: false,
                secret: false,
                allow_other: true,
                options: vec![
                    ferrite_core::questions::Choice {
                        label: "Rewrite".into(),
                        description: "Start over".into(),
                        preview: None,
                    },
                    ferrite_core::questions::Choice {
                        label: "Patch".into(),
                        description: "Smallest change".into(),
                        preview: None,
                    },
                ],
            }]),
            policy: Default::default(),
            id: id.into(),
            tool_use_id: "toolu_q".into(),
            tool_name: "AskUserQuestion".into(),
            description: String::new(),
            input: serde_json::json!({
                "questions": [{
                    "question": "Which approach?",
                    "header": "Approach",
                    "multiSelect": false,
                    "options": [
                        {"label": "Rewrite", "description": "Start over"},
                        {"label": "Patch", "description": "Smallest change"}
                    ]
                }]
            }),
            suggestions: vec![],
        },
    }
}

/// The two Threads of a cockpit, in creation order. Pane order follows a
/// HashMap, so tests about park order must not read it off the grid.
fn created(
    view: &gpui::Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> (ThreadId, ThreadId) {
    view.read_with(cx, |view, _| {
        let mut ids: Vec<ThreadId> = view.panes.iter().filter_map(|pane| pane.thread()).collect();
        ids.sort();
        (ids[0], ids[1])
    })
}

/// A repo with one commit — `git worktree add` needs a commit to branch
/// from, so a bare init is not enough.
/// A git repo with one root commit, inside an already-scratched base.
fn repo_in(base: &std::path::Path) -> std::path::PathBuf {
    let dir = base.join("repo");
    std::fs::create_dir_all(&dir).unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "operator@example.invalid"],
        vec!["config", "user.name", "operator"],
        vec!["commit", "-q", "--allow-empty", "-m", "root"],
    ] {
        let out = std::process::Command::new("git")
            .args(&args)
            .current_dir(&dir)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }
    dir
}

/// The band's key path in the ComposerMenu grammar the tests must bind
/// themselves — production loads it from the keymap table.
fn bind_band_keys(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("enter", Submit, None),
            KeyBinding::new("tab", BandCycle, None),
            KeyBinding::new("up", MenuPrevious, Some("ComposerMenu")),
            KeyBinding::new("down", MenuNext, Some("ComposerMenu")),
            KeyBinding::new("enter", MenuPick, Some("ComposerMenu")),
            KeyBinding::new("escape", MenuDismiss, Some("ComposerMenu")),
        ]);
    });
}

/// A repo on `trunk` with `main` and `topic/spare` as plain branches
/// and one hand-made worktree, `wt-feature`, on `feature/wt` — every
/// shape the workspace chip must tell apart, normalised off whatever
/// this machine's `git init` called its first branch.
fn repo_with_worktree_and_branches(base: &std::path::Path) -> std::path::PathBuf {
    let repo = repo_in(base);
    let worktree = base.join("wt-feature");
    let worktree = worktree.to_str().unwrap();
    for args in [
        vec!["branch", "-M", "trunk"],
        vec!["branch", "main"],
        vec!["branch", "topic/spare"],
        vec!["worktree", "add", "-q", worktree, "-b", "feature/wt"],
    ] {
        let out = std::process::Command::new("git")
            .args(&args)
            .current_dir(&repo)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }
    repo
}

fn step(cx: &mut gpui::VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

/// The on-screen position of a byte in one Block's first text run in
/// the first Pane — where a test aims the mouse (#27). TextLayout
/// records screen-space geometry, so no scroll math is needed.
fn caret(
    view: &gpui::Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    block: usize,
    byte: usize,
) -> gpui::Point<gpui::Pixels> {
    cx.update(|window, cx| {
        let view = view.read(cx);
        let thread = view.panes[0].thread().unwrap();
        let blocks = view
            .cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&view.panes[0].selected)
            .unwrap()
            .transcript()
            .blocks();
        let current = &blocks[block];
        let (id, item, paragraphs, text) = if current.markdown.is_some() {
            let start = (0..=block)
                .rev()
                .take_while(|index| blocks[*index].markdown.is_some())
                .last()
                .unwrap();
            let text = match &current.body {
                ferrite_core::transcript::Body::Paragraph { spans } => spans
                    .iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>(),
                _ => current.markdown.clone().unwrap(),
            };
            (
                format!(
                    "markdown-{}-{:?}",
                    view.panes[0].text_namespace(),
                    current.markdown_run.unwrap_or(blocks[start].id)
                ),
                block - start,
                blocks[start..]
                    .iter()
                    .take_while(|block| block.markdown.is_some())
                    .count(),
                text,
            )
        } else {
            let text = view
                .selection
                .registered(thread)
                .into_iter()
                .find(|(id, ordinal, _, _)| *id == current.id && *ordinal == 0)
                .unwrap()
                .3;
            (
                format!(
                    "literal-{}-{:?}-0",
                    view.panes[0].text_namespace(),
                    current.id
                ),
                0,
                1,
                text,
            )
        };
        ferrite_pane::rich::testing::caret(&id, item, paragraphs, &text, byte, window, cx)
            .expect("a rendered native caret")
    })
}

fn clipboard(cx: &mut gpui::VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

/// A repeated press at the same spot, as the platform reports it: the
/// second or third press of a multi-click carries its count.
fn press(cx: &mut gpui::VisualTestContext, position: gpui::Point<Pixels>, count: usize) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: gpui::Modifiers::none(),
        click_count: count,
        first_mouse: false,
    });
}

/// The production key table, loaded whole in the mac spelling — so the
/// popovers' same-depth tie-breaks are tested against exactly the order
/// launch binds.
fn bind_production_keys(cx: &mut TestAppContext) {
    cx.update(|cx| {
        // Launch initializes the toolkit before binding Ferrite’s keys.
        ferrite_pane::pane::init_components(cx);
        let table = ferrite_ui::keymap::bindings(ferrite_ui::keymap::Platform::Mac);
        let bindings = ferrite_frame::keys::load(table, cx);
        cx.bind_keys(bindings);
    });
}

/// The comp's own slash-menu rows (PromptBox state 02), as a Session
/// would announce them.
fn menu_commands() -> Vec<ferrite_core::SessionCommand> {
    [
        ("code-review", "review branch vs main"),
        ("commit", "stage + commit this pane's diff"),
        ("compact", "summarize context"),
        ("to-tickets", "plan → GitHub issues"),
    ]
    .into_iter()
    .map(|(name, description)| ferrite_core::SessionCommand {
        name: name.into(),
        description: description.into(),
        path: None,
    })
    .collect()
}

/// A binding checkout holding a couple of plain files for the `@` menu.
fn checkout_with_files(base: &std::path::Path) -> std::path::PathBuf {
    let checkout = base.join("checkout");
    std::fs::create_dir_all(checkout.join("src")).unwrap();
    std::fs::write(checkout.join("README.md"), "r\n").unwrap();
    std::fs::write(checkout.join("src").join("lib.rs"), "l\n").unwrap();
    checkout
}

/// One Thread of `provider` bound to a checkout with files to mention.
fn bound_cockpit(name: &str, provider: Provider) -> (Cockpit, Fake, std::path::PathBuf) {
    let base = scratch(name);
    let checkout = checkout_with_files(&base);
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let mut cockpit = Cockpit::new(store, Box::new(fake.clone()));
    cockpit
        .open(
            provider,
            WorkspaceChoice::Main {
                checkout: checkout.clone(),
            },
        )
        .unwrap();
    (cockpit, fake, checkout)
}

fn composer_text(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, cx| {
        view.panes[0].composer.read(cx).text().to_string()
    })
}

fn drag_nav(cx: &mut gpui::VisualTestContext, source: &'static str, target: &'static str) {
    let source = cx.debug_bounds(source).unwrap();
    let target = cx.debug_bounds(target).unwrap();
    let source = gpui::point(source.right() - px(5.), source.center().y);
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(
        gpui::point(source.x - px(30.), source.y),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_move(
        target.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_up(
        target.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
}
