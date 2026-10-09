//! The live reference (`--live-reference <outdir>`): the production Spawner
//! over the real provider CLIs, driven headlessly through one scripted
//! scenario on a throwaway repo and store, and captured at the moments the
//! prototype's features show (`live-<moment>.png`), with a log of what each
//! step saw (`live-log.txt`).
//!
//! Never the operator's store or repos: the store is `FERRITE_STORE`, which
//! must sit under the system temp directory, and the repo is
//! `FERRITE_LIVE_REPO`, a throwaway git checkout there too. Sessions start
//! with permissions that ask — Claude in `default` mode, Codex read-only on
//! request — so Decisions arrive and are answered here with the real keys
//! and clicks. The models are the cheap ones (`FERRITE_LIVE_CLAUDE_MODEL`,
//! default `haiku`; `FERRITE_LIVE_CODEX_MODEL`, default `gpt-6-luna`).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ferrite_core::cockpit::{Cockpit, ProviderChoice};
use ferrite_core::settings::Settings;
use ferrite_core::store::{Provider, Store};
use ferrite_core::transcript::{Body, ToolState};
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;
use gpui::{
    AnyWindowHandle, AppContext as _, Context, Entity, Focusable as _, HeadlessAppContext,
    Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput,
    Point, Styled as _, Window,
};

use crate::cockpit::{CockpitView, Preferences};
use crate::palette::PaletteScope;
use crate::transcript::ScrollTarget;

/// What a step waits on before it acts or shoots.
type Probe = dyn Fn(&CockpitView) -> bool;

struct Live {
    cx: HeadlessAppContext,
    window: AnyWindowHandle,
    view: Entity<CockpitView>,
    out: PathBuf,
    log: std::fs::File,
    started: Instant,
}

impl Live {
    fn note(&mut self, line: impl AsRef<str>) {
        let line = format!(
            "[{:>6.1}s] {}",
            self.started.elapsed().as_secs_f32(),
            line.as_ref()
        );
        eprintln!("{line}");
        let _ = writeln!(self.log, "{line}");
    }

    /// One slice of real time, mirrored on the executor's clock: the pump
    /// runs, the frames the last draw asked for are delivered, a frame is
    /// drawn.
    fn tick(&mut self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
        self.cx.advance_clock(Duration::from_millis(ms));
        self.cx.run_until_parked();
        self.cx
            .update_window(self.window, |_, window, cx| {
                window.simulate_next_frame(cx);
                let _ = window.draw(cx);
            })
            .expect("the live window stands");
    }

    fn read<R>(&mut self, f: impl FnOnce(&CockpitView) -> R) -> R {
        let view = self.view.clone();
        self.cx.update(|cx| f(view.read(cx)))
    }

    fn act<R>(
        &mut self,
        f: impl FnOnce(&mut CockpitView, &mut Window, &mut Context<CockpitView>) -> R,
    ) -> R {
        let view = self.view.clone();
        let out = self
            .cx
            .update_window(self.window, |_, window, cx| {
                view.update(cx, |view, cx| f(view, window, cx))
            })
            .expect("the live window stands");
        self.tick(30);
        out
    }

    /// Ticks until `probe` holds or `secs` pass; true when it held.
    fn wait(&mut self, what: &str, secs: u64, probe: &Probe) -> bool {
        let start = Instant::now();
        loop {
            self.tick(60);
            if self.read(|view| probe(view)) {
                self.note(format!(
                    "ok   {what} ({:.1}s)",
                    start.elapsed().as_secs_f32()
                ));
                return true;
            }
            if start.elapsed() > Duration::from_secs(secs) {
                self.note(format!("FAIL {what}: not within {secs}s"));
                return false;
            }
        }
    }

    /// Settle the frame and save `live-<name>.png`.
    fn shot(&mut self, name: &str) {
        for _ in 0..6 {
            self.tick(60);
        }
        self.cx
            .update_window(self.window, |_, window, cx| {
                let _ = window.draw(cx);
                let _ = window.draw(cx);
            })
            .expect("the live window stands");
        let path = self.out.join(format!("live-{name}.png"));
        match self.cx.capture_screenshot(self.window) {
            Ok(image) => match image.save(&path) {
                Ok(()) => self.note(format!("shot {}", path.display())),
                Err(e) => self.note(format!("FAIL shot {name}: {e}")),
            },
            Err(e) => self.note(format!("FAIL shot {name}: {e}")),
        }
    }

    fn key(&mut self, keystroke: &str) {
        let parsed = Keystroke::parse(keystroke).expect("a keystroke");
        let handled = self
            .cx
            .update_window(self.window, |_, window, cx| {
                window.dispatch_keystroke(parsed, cx)
            })
            .expect("the live window stands");
        self.note(format!(
            "key  {keystroke}{}",
            if handled { "" } else { " (unhandled)" }
        ));
        self.tick(60);
    }

    /// Words typed key by key into whatever holds the keyboard.
    fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            let key = match ch {
                ' ' => "space".to_string(),
                ch => ch.to_string(),
            };
            let parsed = Keystroke::parse(&key).expect("a typed key");
            self.cx
                .update_window(self.window, |_, window, cx| {
                    window.dispatch_keystroke(parsed, cx)
                })
                .expect("the live window stands");
            self.cx.run_until_parked();
        }
        self.note(format!("type {text:?}"));
        self.tick(60);
    }

    fn bounds(&mut self, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
        let selector = selector.to_string();
        self.cx
            .update_window(self.window, |_, window, _| window.debug_bounds(&selector))
            .ok()
            .flatten()
    }

    fn mouse(&mut self, event: PlatformInput) {
        self.cx
            .update_window(self.window, |_, window, cx| {
                window.dispatch_event(event, cx);
            })
            .expect("the live window stands");
        self.tick(30);
    }

    /// A left click at the centre of what `selector` drew, the pointer
    /// moved there first so hover state is honest. False when the last
    /// frame drew no such element.
    fn click(&mut self, selector: &str, modifiers: Modifiers) -> bool {
        let Some(bounds) = self.bounds(selector) else {
            self.note(format!("FAIL click {selector}: not drawn"));
            return false;
        };
        let at = bounds.center();
        self.click_at(at, modifiers);
        self.note(format!(
            "click {selector} at ({:.0}, {:.0})",
            f32::from(at.x),
            f32::from(at.y)
        ));
        true
    }

    fn click_at(&mut self, at: Point<gpui::Pixels>, modifiers: Modifiers) {
        self.mouse(PlatformInput::MouseMove(MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers,
        }));
        self.mouse(PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers,
            click_count: 1,
            first_mouse: false,
        }));
        self.mouse(PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers,
            click_count: 1,
        }));
    }

    /// Put `text` on the focused Composer and send it with ⏎.
    fn send(&mut self, text: &str) {
        self.act(|view, window, cx| {
            let composer = view.panes[view.focused()].composer.clone();
            composer.update(cx, |composer, cx| composer.set(text.to_string(), cx));
            let focus = composer.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        });
        self.note(format!("send {text:?}"));
        self.key("enter");
    }

    fn scroll_bottom(&mut self, thread: ThreadId) {
        self.act(|view, _, cx| {
            if let Some(index) = view.pane_for(thread) {
                view.scroll_transcript_to_bottom(index, cx);
            }
        });
    }

    /// What `thread` said, row by row, as the log keeps it: the evidence
    /// behind each shot's rows.
    fn dump(&mut self, thread: ThreadId, label: &str) {
        let lines = self.read(|view| {
            let Some(open) = view.cockpit.thread(thread) else {
                return vec!["(not open)".to_string()];
            };
            let transcript = open.transcript();
            let mut lines = vec![format!(
                "model={:?} effort={:?} mode={:?} turns={} started={:?} working_ms={:?} usage={:?} cost={:?} limits={:?}",
                open.model(),
                open.effort(),
                open.permission_mode(),
                transcript.turn_count(),
                transcript.started_at(),
                transcript.last_working_ms(),
                transcript.usage(),
                transcript.last_cost(),
                transcript.rate_limits(),
            )];
            for block in transcript.blocks() {
                let line = match &block.body {
                    Body::Prompt(text) => format!("prompt {:?} sent_at={:?}", text, block.sent_at),
                    Body::Tool(tool) => format!(
                        "tool {}({}) state={:?} result={:?} progress={:?} diffs={}",
                        ferrite_core::transcript::display_tool_name(&tool.name),
                        tool.summary,
                        tool.state,
                        tool.result_line,
                        tool.progress,
                        tool.diffs.len()
                    ),
                    Body::TurnEnd(end) => format!("turn-end {:?}", end.text()),
                    Body::Notice(text) => format!("notice {text:?}"),
                    Body::Meta(text) => format!("meta {text:?}"),
                    Body::Thinking(text) => format!("thinking ({} chars)", text.len()),
                    other => {
                        let text = match other {
                            Body::Paragraph { spans }
                            | Body::Bullet { spans }
                            | Body::Heading { spans, .. } => {
                                spans.iter().map(|span| span.text.as_str()).collect()
                            }
                            Body::Code { source, .. } => source.clone(),
                            _ => String::new(),
                        };
                        let mut text: String = text.chars().take(100).collect();
                        text = text.replace('\n', " ");
                        format!("text {text:?}")
                    }
                };
                lines.push(line);
            }
            lines
        });
        self.note(format!("---- {label}: thread {thread}"));
        for line in lines {
            self.note(format!("     {line}"));
        }
    }
}

fn waiting(view: &CockpitView, thread: ThreadId) -> bool {
    view.cockpit.thread(thread).is_some_and(|open| {
        open.activity()
            .pending_decisions()
            .iter()
            .any(|request| !request.submitting)
    })
}

fn turns(view: &CockpitView, thread: ThreadId) -> usize {
    view.cockpit
        .thread(thread)
        .map(|open| open.transcript().turn_count())
        .unwrap_or(0)
}

fn busy(view: &CockpitView, thread: ThreadId) -> bool {
    view.cockpit.thread(thread).is_some_and(|open| open.busy())
}

/// A tool call still running, and how far its test count got.
fn running_tool(view: &CockpitView, thread: ThreadId) -> Option<Option<(u32, u32)>> {
    let open = view.cockpit.thread(thread)?;
    open.transcript()
        .blocks()
        .iter()
        .rev()
        .find_map(|block| match &block.body {
            Body::Tool(tool) if tool.state == ToolState::Running => {
                Some(tool.progress.map(|p| (p.done, p.total)))
            }
            _ => None,
        })
}

fn temp_root() -> PathBuf {
    std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// Refuses any path outside the temp directories: the operator's store
/// and repos are never touched.
fn throwaway(var: &str, default: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var(var).unwrap_or_else(|_| default.to_string()));
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
    let allowed = [
        temp_root(),
        PathBuf::from("/private/tmp"),
        PathBuf::from("/tmp"),
    ];
    if !allowed.iter().any(|root| canonical.starts_with(root)) {
        eprintln!(
            "ferrite: {var} must be a throwaway path under the temp directory, not {}",
            path.display()
        );
        std::process::exit(2);
    }
    canonical
}

pub(crate) fn capture(output: String) {
    let out = PathBuf::from(output);
    std::fs::create_dir_all(&out).expect("create artifact directory");
    let repo = throwaway("FERRITE_LIVE_REPO", "/tmp/ferrite-live/repo");
    let store_dir = throwaway("FERRITE_STORE", "/tmp/ferrite-live/store");
    let settings_dir = store_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| store_dir.clone())
        .join("settings");
    let claude_model =
        std::env::var("FERRITE_LIVE_CLAUDE_MODEL").unwrap_or_else(|_| "haiku".into());
    let codex_model =
        std::env::var("FERRITE_LIVE_CODEX_MODEL").unwrap_or_else(|_| "gpt-6-luna".into());
    let codex_effort = std::env::var("FERRITE_LIVE_CODEX_EFFORT").unwrap_or_else(|_| "low".into());
    // The Thread's workspace is the throwaway repo; Ferrite stands there,
    // as a launch from inside it would.
    std::env::set_current_dir(&repo).expect("stand in the throwaway repo");

    let mut settings = Settings::default();
    settings.default_provider = Provider::Claude;
    settings.claude_model = Some(claude_model.clone());
    settings.codex_model = Some(codex_model.clone());
    settings.codex_effort = Some(codex_effort.clone());
    // Permissions that ask, whatever the operator's CLIs default to.
    settings.claude_permission_mode = Some("default".into());
    settings.codex_approval_policy = "on-request".into();
    settings.codex_sandbox = Some("read-only".into());
    // No prediction turns: only what the scenario asks for is spent.
    settings.placeholder_suggestions = false;
    let defaults = std::sync::Arc::new(std::sync::Mutex::new(
        crate::session::SessionDefaults::from_settings(&settings),
    ));
    let mut core = Cockpit::try_new(
        Store::open(store_dir.clone()).expect("open the throwaway store"),
        Box::new(crate::session::Spawn::new(defaults.clone(), None)),
    )
    .expect("open the throwaway registry");
    core.set_suggestions_enabled(false);
    let project = core
        .register_project(&repo)
        .expect("register the throwaway repo");

    let platform = gpui::platform::current_platform(true);
    let mut cx = HeadlessAppContext::with_platform(
        platform.text_system(),
        std::sync::Arc::new(crate::icons::Assets),
        gpui::platform::current_headless_renderer,
    );
    cx.update(|cx| {
        crate::theme::init_components(cx);
        crate::motion::init(cx);
        crate::register_fonts(cx);
        let bindings = crate::load_bindings(crate::keymap::PLATFORM, cx);
        cx.bind_keys(bindings);
    });
    let mut entity = None;
    let prefs = Preferences {
        settings,
        dir: settings_dir,
        defaults,
        titler: true,
        cli_updates: false,
    };
    let window = cx
        .open_window(gpui::size(gpui::px(1440.), gpui::px(900.)), |window, cx| {
            let view =
                cx.new(|cx| CockpitView::new_with_settings(core, Provider::Claude, prefs, cx));
            entity = Some(view.clone());
            let content = cx.new(|cx| crate::loops_overlay::CockpitWindow::new(view, window, cx));
            cx.new(|cx| {
                gpui::component::Root::new(content, window, cx)
                    .bordered(false)
                    .bg(crate::theme::paint::WINDOW)
            })
        })
        .expect("open the live window");
    let view = entity.expect("the window built its view");
    let log = std::fs::File::create(out.join("live-log.txt")).expect("create the live log");
    let mut live = Live {
        cx,
        window: window.into(),
        view,
        out,
        log,
        started: Instant::now(),
    };
    live.note(format!(
        "live reference: repo {} store {} claude {claude_model} codex {codex_model}/{codex_effort}",
        repo.display(),
        store_dir.display()
    ));
    live.act(|view, window, cx| {
        view.set_launch_dir(repo.clone());
        window.activate_window();
        cx.notify();
    });

    let phases = std::env::var("FERRITE_LIVE_PHASES").unwrap_or_else(|_| "all".into());
    let wants = |phase: &str| phases == "all" || phases.split(',').any(|p| p == phase);

    let claude = scenario(
        &mut live,
        project,
        &claude_model,
        &codex_model,
        &codex_effort,
        &wants,
    );

    // Every Session ends with the capture: no CLI outlives it.
    live.act(|view, _, _| view.cockpit.halt_sessions());
    for _ in 0..10 {
        live.tick(50);
    }
    live.note(format!("done; claude thread {claude:?}"));
    let Live { cx, view, .. } = live;
    drop(view);
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(cx)));
    std::panic::set_hook(hook);
}

/// The draft on the focused Pane, aimed at `provider`'s cheap model on the
/// throwaway repo's checkout.
fn aim_draft(
    live: &mut Live,
    project: ProjectId,
    provider: Provider,
    model: &str,
    effort: Option<&str>,
) {
    let model = model.to_string();
    let effort = effort.map(str::to_string);
    live.act(move |view, _, cx| {
        let announced = view.cockpit.announced_models(provider);
        if let Some(draft) = view.focused_draft_mut() {
            draft.binding.choose_checkout(project);
            draft.binding.choose_provider(
                ProviderChoice {
                    provider,
                    model: Some(model),
                },
                &announced,
            );
            draft.binding.choose_effort(effort);
        }
        cx.notify();
    });
}

fn focused_thread(live: &mut Live) -> Option<ThreadId> {
    live.read(|view| view.focused_thread())
}

/// The tool a standing request of `thread` asks for, if one stands.
fn asking(view: &CockpitView, thread: ThreadId) -> Option<String> {
    view.cockpit.thread(thread).and_then(|open| {
        open.activity()
            .pending_decisions()
            .iter()
            .find(|request| !request.submitting)
            .map(|request| request.decision.tool_name.clone())
    })
}

fn mode(view: &CockpitView, thread: ThreadId) -> Option<String> {
    view.cockpit
        .thread(thread)
        .and_then(|open| open.permission_mode().map(str::to_owned))
}

/// Waits until `thread` asks something or its turn `turn` is over; true
/// when it asks.
fn asks_or_ends(live: &mut Live, what: &str, thread: ThreadId, turn: usize, secs: u64) -> bool {
    live.wait(what, secs, &move |view| {
        waiting(view, thread) || (turns(view, thread) >= turn && !busy(view, thread))
    });
    live.read(|view| waiting(view, thread))
}

/// Waits for `thread`'s running call to stream a test count past a fifth
/// of its total, and shoots it. False when the call settled first.
fn shoot_running_suite(live: &mut Live, thread: ThreadId, turn: usize, name: &str) -> bool {
    // A known total a fifth done, or (Codex lost the announcement) a count
    // of a few dozen; a request or the turn's end stops the wait.
    let counting = move |view: &CockpitView| {
        matches!(running_tool(view, thread), Some(Some((done, total)))
            if (total > 0 && done * 5 > total && done < total) || (total == 0 && done > 40))
    };
    let counted = live.wait(&format!("{name}: suite counting"), 120, &move |view| {
        counting(view)
            || waiting(view, thread)
            || (turns(view, thread) >= turn && !busy(view, thread))
    });
    if counted && live.read(counting) {
        live.shot(name);
        live.dump(thread, name);
        return true;
    }
    live.note(format!(
        "FAIL {name}: no streamed count before the call settled"
    ));
    false
}

/// The pointer on the first laid-out target naming `path` in `thread`'s
/// transcript: the real hover the card opens on. False when none is drawn.
fn hover_path(live: &mut Live, thread: ThreadId, path: &str) -> Option<Point<gpui::Pixels>> {
    let path = PathBuf::from(path);
    let anchor = live.cx.update(|cx| {
        let view = live.view.read(cx);
        let index = view.pane_for(thread)?;
        let transcript = view.panes[index].transcript()?;
        let scope = transcript.read(cx).namespace();
        crate::file_links::laid_out(&scope, &path, cx)
    });
    let Some(anchor) = anchor else {
        live.note(format!(
            "FAIL hover: no target naming {} laid out",
            path.display()
        ));
        return None;
    };
    let at = anchor.center();
    live.mouse(PlatformInput::MouseMove(MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers: Modifiers::default(),
    }));
    live.note(format!(
        "hover {} at ({:.0}, {:.0})",
        path.display(),
        f32::from(at.x),
        f32::from(at.y)
    ));
    Some(at)
}

/// The pointer parked over the nav's empty foot, where nothing hovers.
fn rest_pointer(live: &mut Live) {
    live.mouse(PlatformInput::MouseMove(MouseMoveEvent {
        position: gpui::point(gpui::px(150.), gpui::px(820.)),
        pressed_button: None,
        modifiers: Modifiers::default(),
    }));
}

/// Focus `thread`'s Pane and its Composer, as a click on it would.
fn focus_thread(live: &mut Live, thread: ThreadId) {
    live.act(move |view, window, cx| {
        if let Some(index) = view.pane_for(thread) {
            view.cockpit.focus_thread(thread);
            view.focus_pane(index);
            let focus = view.panes[index].composer.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    });
}

fn scenario(
    live: &mut Live,
    project: ProjectId,
    claude_model: &str,
    codex_model: &str,
    codex_effort: &str,
    wants: &dyn Fn(&str) -> bool,
) -> Option<ThreadId> {
    // ---- launch on an empty store
    live.shot("empty");

    // ---- Solo: a Claude Thread from a draft, its first Decision
    live.key("cmd-n");
    aim_draft(live, project, Provider::Claude, claude_model, None);
    live.send(
        "Run `cargo test` in this repo and tell me in one sentence which test fails. Do not edit any files.",
    );
    if !live.wait("claude thread started", 60, &|view| {
        view.focused_thread().is_some()
    }) {
        return None;
    }
    let claude = focused_thread(live)?;
    if asks_or_ends(live, "claude asks to run cargo test", claude, 1, 180) {
        live.shot("claude-decision");
        live.key("down");
        live.shot("claude-decision-cursor2");
        live.key("up");
        // 1: allow.
        live.key("1");
        shoot_running_suite(live, claude, 1, "claude-running");
    }
    live.wait("claude turn 1 ends", 300, &move |view| {
        turns(view, claude) >= 1 && !busy(view, claude)
    });
    live.scroll_bottom(claude);
    live.shot("claude-turn1");
    live.dump(claude, "claude after turn 1");

    if wants("fix") {
        // ---- turn two: the edit denied with a note (↓↓ ⇥ note ⏎), then
        // edits allowed for the thread (2), the rerun allowed (1).
        live.send(
            "Fix the bug in src/stats.rs so the even-length median test passes (do not touch the tests), then run `cargo test` again.",
        );
        let mut noted = false;
        for round in 0..6 {
            if !asks_or_ends(
                live,
                &format!("claude turn 2 asks ({round})"),
                claude,
                2,
                240,
            ) {
                break;
            }
            let tool = live.read(|view| asking(view, claude)).unwrap_or_default();
            live.shot(&format!("claude-ask-{round}-{}", tool.to_lowercase()));
            match tool.as_str() {
                "Edit" | "MultiEdit" | "Write" if !noted => {
                    noted = true;
                    live.key("down");
                    live.key("down");
                    live.key("tab");
                    live.type_text("average the two middle values");
                    live.shot("claude-amend");
                    live.key("enter");
                }
                "Edit" | "MultiEdit" | "Write" => live.key("2"),
                _ => {
                    live.key("1");
                    shoot_running_suite(live, claude, 2, "claude-rerun-running");
                }
            }
        }
        live.wait("claude turn 2 ends", 300, &move |view| {
            turns(view, claude) >= 2 && !busy(view, claude)
        });
        live.scroll_bottom(claude);
        live.shot("claude-turn2");
        live.act(move |view, _, cx| {
            if let Some(index) = view.pane_for(claude) {
                view.scroll_transcript(index, ScrollTarget::Top, cx);
            }
        });
        live.shot("claude-top");
        live.dump(claude, "claude after turn 2");
        live.scroll_bottom(claude);
        for _ in 0..6 {
            live.tick(60);
        }

        // ---- the path preview, hovered for real, and ⌘-click's reader
        let edited = live.read(move |view| {
            let root = view
                .cockpit
                .thread(claude)
                .and_then(|open| open.workspace().map(|w| w.cwd().to_path_buf()));
            view.cockpit.thread(claude).and_then(|open| {
                open.transcript()
                    .blocks()
                    .iter()
                    .rev()
                    .find_map(|block| match &block.body {
                        Body::Tool(tool) => tool.diffs.first().map(|diff| {
                            root.as_ref()
                                .and_then(|root| Path::new(&diff.path).strip_prefix(root).ok())
                                .map(|rest| rest.to_string_lossy().into_owned())
                                .unwrap_or_else(|| diff.path.clone())
                        }),
                        _ => None,
                    })
            })
        });
        let path = edited.unwrap_or_else(|| "src/stats.rs".into());
        let mut at = hover_path(live, claude, &path);
        if at.is_none() {
            live.act(move |view, _, cx| {
                if let Some(index) = view.pane_for(claude) {
                    view.scroll_transcript(
                        index,
                        ScrollTarget::TurnBand {
                            turn: 1,
                            offset: 0.,
                        },
                        cx,
                    );
                }
            });
            for _ in 0..6 {
                live.tick(60);
            }
            at = hover_path(live, claude, &path);
        }
        if let Some(at) = at {
            live.shot("claude-hover");
            live.click_at(
                at,
                Modifiers {
                    platform: true,
                    ..Default::default()
                },
            );
            live.note("cmd-click on the hovered path");
            for _ in 0..8 {
                live.tick(60);
            }
            rest_pointer(live);
            live.shot("claude-reader");
            live.act(move |view, _, cx| {
                if let Some(index) = view.pane_for(claude) {
                    view.close_reader(index, cx);
                }
            });
        }
        rest_pointer(live);
        live.scroll_bottom(claude);
    }

    // ---- the model picker, the shortcuts sheet, ⇧⇥
    live.act(move |view, _, cx| view.open_provider_picker(claude, cx));
    live.shot("claude-picker");
    live.act(|view, _, cx| {
        view.popover = None;
        cx.notify();
    });
    focus_thread(live, claude);
    live.key("?");
    live.shot("shortcuts");
    live.key("escape");
    // ⇧⇥ round the cycle, stopping on `default` so the next request asks.
    let start = live.read(move |view| mode(view, claude));
    live.note(format!("mode {start:?}"));
    for step in 0..5 {
        live.key("shift-tab");
        for _ in 0..4 {
            live.tick(60);
        }
        let now = live.read(move |view| mode(view, claude));
        live.note(format!("mode after shift-tab {step}: {now:?}"));
        if step == 0 {
            live.shot("claude-mode-cycled");
        }
        if now.as_deref() == Some("default") {
            break;
        }
    }

    if wants("esc") {
        // ---- esc denies a Claude request
        live.send("Delete README.md.");
        if asks_or_ends(live, "claude asks to delete", claude, 3, 180) {
            live.shot("claude-esc-decision");
            live.key("escape");
        }
        live.wait("claude turn 3 ends", 240, &move |view| {
            turns(view, claude) >= 3 && !busy(view, claude)
        });
        live.scroll_bottom(claude);
        live.shot("claude-turn3");
        live.dump(claude, "claude after turn 3");
    }

    if !wants("group") {
        return Some(claude);
    }
    // ---- Group: ⌘⇧G founds a Group with a Codex Thread beside Claude.
    live.key("cmd-shift-g");
    aim_draft(
        live,
        project,
        Provider::Codex,
        codex_model,
        Some(codex_effort),
    );
    live.shot("group-draft");
    live.send(
        "Run `cargo test` in this repo. It has to build, so request escalated (write) permission for it up front. Then tell me in one sentence whether any test fails.",
    );
    if !live.wait("codex thread started", 90, &move |view| {
        view.focused_thread().is_some_and(|thread| thread != claude)
    }) {
        return Some(claude);
    }
    let codex = focused_thread(live)?;
    // Back on Claude, so Codex's requests arrive off focus: toasts.
    focus_thread(live, claude);
    let mut codex_turns = 1;
    let mut allowed = false;
    for round in 0..4 {
        if !asks_or_ends(live, &format!("codex turn 1 asks ({round})"), codex, 1, 240) {
            break;
        }
        live.shot(&format!("group-toast-{round}"));
        live.dump(codex, "codex asking");
        // The toast's `1 allow`, clicked.
        let selector = format!("toast-allow-{}", codex.get());
        if !live.click(&selector, Modifiers::default()) {
            focus_thread(live, codex);
            live.key("1");
            focus_thread(live, claude);
        }
        if !allowed {
            allowed = shoot_running_suite(live, codex, 1, "group-codex-running");
        }
    }
    live.wait("codex turn 1 ends", 300, &move |view| {
        turns(view, codex) >= 1 && !busy(view, codex)
    });
    live.scroll_bottom(codex);
    live.shot("group-turn1");
    live.dump(codex, "codex after turn 1");

    if wants("deny") {
        // ---- the toast's `3 deny`
        focus_thread(live, codex);
        live.send("Create a file NOTES.md containing one line: hello.");
        focus_thread(live, claude);
        codex_turns += 1;
        if asks_or_ends(live, "codex asks to write (toast)", codex, codex_turns, 240) {
            live.shot("group-toast-deny");
            let selector = format!("toast-deny-{}", codex.get());
            if !live.click(&selector, Modifiers::default()) {
                focus_thread(live, codex);
                live.key("3");
            }
            live.shot("group-toast-denied");
        }
        live.wait("codex turn 2 ends", 240, &move |view| {
            turns(view, codex) >= 2 && !busy(view, codex)
        });
        // ---- `2` allows for the Thread, from the Pane
        focus_thread(live, codex);
        live.send("Try again: create NOTES.md containing one line: hello.");
        codex_turns += 1;
        let target = codex_turns;
        for round in 0..3 {
            if !asks_or_ends(
                live,
                &format!("codex asks again ({round})"),
                codex,
                target,
                240,
            ) {
                break;
            }
            live.shot(&format!("codex-ask-{round}"));
            focus_thread(live, codex);
            live.key("2");
        }
        live.wait("codex turn 3 ends", 240, &move |view| {
            turns(view, codex) >= target && !busy(view, codex)
        });
        live.scroll_bottom(codex);
        live.shot("group-turn3");
        live.dump(codex, "codex after turn 3");
        // ---- `3` from the Pane: deny, and the Composer takes the steer
        live.send("Delete NOTES.md.");
        codex_turns += 1;
        let target = codex_turns;
        if asks_or_ends(live, "codex asks to delete", codex, target, 240) {
            live.shot("codex-delete-decision");
            focus_thread(live, codex);
            live.key("3");
            live.shot("codex-denied");
        }
        live.wait("codex turn 4 ends", 240, &move |view| {
            turns(view, codex) >= target && !busy(view, codex)
        });
        live.scroll_bottom(codex);
        live.shot("group-turn4");
        live.dump(codex, "codex after turn 4");
    }

    // ---- the bell's list: its rows, keys and `mark all read`
    focus_thread(live, claude);
    live.key("cmd-i");
    live.shot("notifications");
    live.key("down");
    live.shot("notifications-down");
    live.click("notifications-mark-read", Modifiers::default());
    live.shot("notifications-read");
    live.key("backspace");
    live.shot("notifications-dismissed");
    live.key("enter");
    live.shot("notifications-opened");
    live.key("cmd-i");
    live.key("escape");

    // ---- the palette over the live Threads; ⇥ previews in a Pane
    live.key("cmd-k");
    live.shot("palette");
    live.type_text("fail");
    live.shot("palette-typed");
    // ⇥: the highlighted Thread previewed in a Pane beside the current one.
    live.key("down");
    live.key("tab");
    for _ in 0..6 {
        live.tick(60);
    }
    live.shot("palette-preview");
    live.key("escape");
    focus_thread(live, claude);
    live.act(|view, window, cx| view.open_palette(PaletteScope::Groups, "", window, cx));
    live.shot("palette-groups");
    live.key("escape");

    // ---- the Codex model picker and its effort
    live.act(move |view, _, cx| view.open_provider_picker(codex, cx));
    live.shot("codex-picker");
    live.act(|view, _, cx| {
        view.popover = None;
        cx.notify();
    });

    // ---- the wall: this run's Threads beside the CLI sessions this repo
    // already holds, adopted as parked Threads; Codex asks meanwhile, and
    // its tile's `3` answers.
    if wants("wall") {
        let group = live.read(move |view| view.cockpit.groups().of(codex).map(|group| group.id));
        let adopted = adopt_sessions(live);
        if let Some(group) = group {
            live.act(move |view, _, cx| {
                for thread in adopted {
                    let _ = view
                        .cockpit
                        .apply_group(ferrite_core::groups::GroupChange::Join {
                            thread,
                            group,
                            index: None,
                        });
                }
                view.enter_group(group, cx);
                view.sync_panes(cx);
            });
            focus_thread(live, codex);
            live.send("Delete README.md.");
            codex_turns += 1;
            let target = codex_turns;
            focus_thread(live, claude);
            if asks_or_ends(live, "codex asks on the wall", codex, target, 240) {
                live.shot("wall");
                live.key("3");
                live.shot("wall-answered");
            } else {
                live.shot("wall");
            }
            live.wait("codex wall turn ends", 240, &move |view| {
                turns(view, codex) >= target && !busy(view, codex)
            });
            live.shot("wall-settled");
        }
    }

    live.key("cmd-b");
    for _ in 0..6 {
        live.tick(60);
    }
    live.shot("collapsed");
    live.key("cmd-b");

    // ---- the empty board over this store: its real recent Threads and
    // the CLIs found on this machine
    live.act(|view, _, cx| view.show_empty_board(cx));
    for _ in 0..20 {
        live.tick(100);
    }
    live.shot("empty-board");
    live.dump(claude, "claude final");
    live.dump(codex, "codex final");
    Some(claude)
}

/// The CLI sessions this repo already holds (earlier live runs, probes),
/// adopted as parked Threads: real histories for the wall's other tiles.
fn adopt_sessions(live: &mut Live) -> Vec<ThreadId> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let repo = std::env::current_dir().unwrap_or_default();
    let slug: String = repo
        .to_string_lossy()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let mut files: Vec<PathBuf> = std::fs::read_dir(home.join(".claude/projects").join(&slug))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    let codex_day = home.join(".codex/sessions");
    let mut stack = vec![codex_day];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "jsonl")
                && std::fs::read_to_string(&path)
                    .map(|text| {
                        text.lines()
                            .next()
                            .is_some_and(|first| first.contains(&*repo.to_string_lossy()))
                    })
                    .unwrap_or(false)
            {
                files.push(path);
            }
        }
    }
    let live_sessions: Vec<String> = live.read(|view| {
        view.cockpit
            .threads()
            .into_iter()
            .filter_map(|thread| {
                view.cockpit
                    .thread(thread)
                    .and_then(|open| open.transcript().session_id().map(str::to_owned))
            })
            .collect()
    });
    files.retain(|file| {
        !live_sessions
            .iter()
            .any(|id| file.to_string_lossy().contains(id.as_str()))
    });
    files.sort();
    files.truncate(7);
    let mut adopted = Vec::new();
    for file in files {
        let result = live.act(|view, _, _| view.cockpit.import(&file));
        match result {
            Ok(thread) => {
                live.note(format!("adopted {} as thread {thread}", file.display()));
                adopted.push(thread);
            }
            Err(error) => live.note(format!("could not adopt {}: {error}", file.display())),
        }
    }
    adopted
}
