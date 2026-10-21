// Ferrite: the cockpit window and the pump behind it.
// A GUI app, not a console one: without this Windows hands ferrite.exe its
// own console window at launch. Left on in debug builds so `cargo run`
// still prints.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod attachment_preview;
mod attachments;
mod background_chips;
#[cfg(unix)]
mod board;
mod cli_updates;
mod cockpit;
mod components;
mod composer;
mod decision;
mod demo;
mod facts;
mod ferrite_update;
mod file_links;
mod find;
mod float;
mod fuzzy;
mod hover_card;
mod icons;
mod keymap;
mod line;
mod loops_overlay;
mod machines;
mod menu;
mod motion;
mod nav;
mod notifications;
mod palette;
mod pane;
mod platform_text;
mod pointer;
mod pr;
mod prefs;
mod project_editor;
mod projects;
mod prompt_drop;
mod review;
mod rich;
mod scrollbar;
mod select;
mod shortcuts;
#[cfg(unix)]
mod sidebar;
mod status;
mod system_notifications;
mod theme;
mod titlebar;
mod transcript;
mod visual;

use ::gpui;
use ::gpui as kit;
use ::gpui::*;
use ferrite_core::cockpit::Cockpit;
use ferrite_core::server::{settings_dir, store_dir};
use ferrite_core::settings::Settings;
use ferrite_core::store::{Provider, Store};
use ferrite_core::workspace::WorkspaceBinding;
use ferrite_core::ThreadId;

use cockpit::CockpitView;
use ferrite_core::spawner::{self, ProcessRss, SessionDefaults};

actions!(ferrite, [Quit]);

/// Transcript Markdown can show remote images (`![alt](https://…)`); GPUI
/// loads them through the application's HTTP client, whose default refuses
/// every request.
fn http_client() -> std::sync::Arc<dyn gpui::http_client::HttpClient> {
    let agent = concat!("Ferrite/", env!("CARGO_PKG_VERSION"));
    std::sync::Arc::new(
        reqwest_client::ReqwestClient::user_agent(agent)
            .unwrap_or_else(|_| reqwest_client::ReqwestClient::new()),
    )
}

/// GPUI defaults to hiding the OS pointer whenever a key is typed. In a
/// dense cockpit that reads as a flash whenever the pointer is resting over
/// the Composer, so Ferrite keeps pointer visibility under the operator's
/// direct control instead: it moves when the mouse moves and never flickers
/// merely because text changed beneath it.
fn keep_mouse_cursor_visible(cx: &mut App) {
    cx.set_cursor_hide_mode(CursorHideMode::Never);
}

/// The bundled faces, compiled in: Geist Mono for the transcript and code,
/// Geist for the chrome (theme rule 1). gpui has no variation-axis support,
/// so each weight is its own static file. A face's weights share its
/// typographic family name (`theme::FONT_CODE` = Geist Mono,
/// `theme::FONT_UI` = Geist), and CoreText / DirectWrite resolve the right
/// one from `.font_weight(..)` — never reach a weight by family name.
/// Geist Mono comes first: the baseline correction is measured on it.
pub(crate) static FONTS: [&[u8]; 10] = [
    include_bytes!("../assets/fonts/GeistMono.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Italic.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Medium.ttf"),
    include_bytes!("../assets/fonts/GeistMono-SemiBold.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Bold.ttf"),
    include_bytes!("../assets/fonts/Geist.ttf"),
    include_bytes!("../assets/fonts/Geist-Italic.ttf"),
    include_bytes!("../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Geist-Bold.ttf"),
];

/// Registers the bundled faces. Call it before anything lays out text:
/// `add_fonts` returns a Result, and a discarded one fails silently — you get
/// the system font and no explanation.
pub(crate) fn register_fonts(cx: &App) {
    cx.text_system()
        .add_fonts(
            FONTS
                .iter()
                .map(|face| std::borrow::Cow::Owned(platform_text::browser_baseline(face)))
                .collect(),
        )
        .expect("the bundled faces load");
}

fn main() {
    // `ferrite server` is the headless Server the window attaches to (ADR
    // 0025): it never starts a window, Chromium or the self-update.
    if std::env::args().nth(1).as_deref() == Some("server") {
        std::process::exit(ferrite_core::server::run(ferrite_core::server::Options {
            store: store_dir(),
            settings_dir: settings_dir(),
            version: env!("CARGO_PKG_VERSION").into(),
            after: flag(&std::env::args().collect::<Vec<_>>(), "--after")
                .and_then(|pid| pid.parse().ok()),
        }));
    }
    // `ferrite thread list`, `ferrite agent prompt …`, `ferrite --skill`:
    // the automation API's CLI (ADR 0027), which never starts a window.
    if let Some(code) = ferrite_core::api::cli::run(
        &std::env::args().collect::<Vec<_>>(),
        &store_dir(),
        &settings_dir(),
    ) {
        std::process::exit(code);
    }
    // A Chromium helper process is this same binary, started
    // by Chromium with `--type=…`; it runs the helper and exits here, before
    // any shell, store or window. In the app this only readies Chromium
    // (ADR 0013); it starts on the first visual.
    visual::boot();
    // Chromium's headless check, run by CI where it ships (`visual::smoke`).
    #[cfg(feature = "cef")]
    if std::env::args().nth(1).as_deref() == Some("--visual-smoke") {
        std::process::exit(visual::smoke::run());
    }
    // A panic must not take accepted history with it: whatever a writer
    // holds goes into its log before the process unwinds (ADR 0009).
    ferrite_core::store::install_panic_rescue();
    // Before the first glyph is rasterised (gpui caches the answer once):
    // the product draws text as macOS does; only the parity captures turn
    // smoothing off to match the browser-drawn prototype pixel for pixel.
    if std::env::args().nth(1).as_deref() == Some("--visual-reference") {
        platform_text::disable_font_smoothing();
    } else {
        platform_text::follow_system_font_smoothing();
    }
    #[cfg(feature = "visual-reference")]
    if std::env::args().nth(1).as_deref() == Some("--visual-reference") {
        cockpit::visual_reference::capture(
            std::env::args()
                .nth(2)
                .expect("an output directory is required"),
        );
        return;
    }
    // The loops overlay's pixel parity: one scene drawn with and without
    // it at the same instants (`visual_reference::loops_parity`).
    #[cfg(feature = "visual-reference")]
    if std::env::args().nth(1).as_deref() == Some("--loops-parity") {
        cockpit::visual_reference::loops_parity(
            std::env::args()
                .nth(2)
                .expect("an output directory is required"),
        );
        return;
    }
    // Visuals' agent screenshots (`visual::shots::snapshots`).
    #[cfg(feature = "visual-reference")]
    if std::env::args().nth(1).as_deref() == Some("--visual-snapshots") {
        visual::shots::snapshots(
            std::env::args()
                .nth(2)
                .expect("an output directory is required"),
        );
        return;
    }
    // The same capture over the real provider CLIs, on a throwaway repo
    // and store (`visual_reference::live`).
    #[cfg(feature = "visual-reference")]
    if std::env::args().nth(1).as_deref() == Some("--live-reference") {
        cockpit::visual_reference::live_capture(
            std::env::args()
                .nth(2)
                .expect("an output directory is required"),
        );
        return;
    }
    // The operator's settings live beside the store (`~/.ferrite`), and
    // the log and crash reports beside them: opened first, so everything
    // after is on record.
    let settings_dir = settings_dir();
    ferrite_core::diagnostics::open(&settings_dir);
    let args: Vec<String> = std::env::args().collect();
    let load = args.iter().any(|arg| arg == "--load");
    let demo = load || args.iter().any(|arg| arg == "--demo");
    // One Ferrite updates itself: the first window, which holds the update
    // claim for as long as it runs (ADR 0023, 0025). A Ferrite update the
    // last run downloaded installs now, before this window starts a Server
    // from the old executable. A successful install does not return here:
    // the new Ferrite takes this process's place.
    let ferrite_updates =
        ferrite_update::enabled() && !demo && ferrite_update::claim(&settings_dir);
    let whats_new = ferrite_updates
        .then(|| ferrite_update::at_launch(&settings_dir))
        .flatten();
    // Before the args, the store, or any spawn: a Dock launch has no PATH
    // worth the name until the login shell is asked (crate::shell).
    let dock = ferrite_core::login_shell::adopt_login_environment();
    // `--demo parity`: the UI refresh mockups' world, on its own store.
    let parity = flag(&args, "--demo") == Some("parity");
    let (settings, unreadable) = match Settings::load(&settings_dir) {
        Ok(settings) => (settings, None),
        Err(e) => {
            ferrite_core::log!("settings.json could not be read: {e}");
            (Settings::default(), Some(e.to_string()))
        }
    };
    let provider = match flag(&args, "--provider") {
        None => settings.default_provider,
        Some("claude") => Provider::Claude,
        Some("codex") => Provider::Codex,
        Some(other) => {
            eprintln!("ferrite: unknown provider `{other}` (claude, codex)");
            std::process::exit(2);
        }
    };
    // Every --import <path>: a session file from the Claude or Codex CLI,
    // adopted as a Thread of its own.
    let mut imports: Vec<String> = Vec::new();
    for (at, arg) in args.iter().enumerate() {
        if arg != "--import" {
            continue;
        }
        match args.get(at + 1) {
            Some(path) => imports.push(path.clone()),
            // Silently dropping it would look like an import that did nothing.
            None => {
                eprintln!("ferrite: --import needs the path of a session file");
                std::process::exit(2);
            }
        }
    }
    // `--remote <ssh target>`: start on that machine (ADR 0026).
    let remote = flag(&args, "--remote").map(str::to_string);
    if let Some(Err(e)) = remote.as_deref().map(ferrite_core::machine::check_target) {
        eprintln!("ferrite: --remote: {e}");
        std::process::exit(2);
    }
    // How many Panes to open. The perf run uses 24.
    let panes: usize = flag(&args, "--panes")
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);

    // The icons are registered on the application, before `run`:
    // `with_assets` replaces both the asset source and the SVG renderer,
    // and `svg()` finds neither afterwards.
    kit::application()
        .with_assets(icons::Assets)
        .with_http_client(http_client())
        .run(move |cx: &mut App| {
            keep_mouse_cursor_visible(cx);
            theme::init_components(cx);
            motion::init(cx);
            register_fonts(cx);
            visual::init(cx);
            visual::diagram::init(settings.visuals != ferrite_core::visual::Level::Off, cx);

            // A refused key is said by the window (`cockpit::user_commands`).
            let (keys, keys_refused) = keymap::with_user_keys(keymap::PLATFORM, &settings.keys);
            bind_keys(keys, cx);

            let defaults = std::sync::Arc::new(std::sync::Mutex::new(
                SessionDefaults::from_settings(&settings),
            ));
            // The fixture adapter serves `--demo` and `--load`, in this
            // process. Every other launch is a window on the Server, which
            // spawns the real provider CLIs and keeps them when it goes
            // (ADR 0025).
            let (mut core, machines) = if parity {
                (demo::mockup_world(), None)
            } else if demo {
                let store = Store::open(store_dir()).unwrap_or_else(|e| {
                    ferrite_core::log!("cannot open the Thread store: {e}");
                    std::process::exit(1);
                });
                let core = Cockpit::try_new(store, Box::new(demo::Spawn::new(load)))
                    .unwrap_or_else(|e| {
                        ferrite_core::log!("cannot open the workspace registry: {e}");
                        std::process::exit(1);
                    });
                (core, None)
            } else {
                // A released app's Server is launchd's (the Run key's) from
                // its first launch, so the agents' Threads resume at login;
                // a development build starts its own. Each launch rewrites
                // the LaunchAgent when the app moved.
                if settings.start_at_login && !titlebar::DEV {
                    if let Err(e) = std::env::current_exe()
                        .and_then(|exe| ferrite_core::server::agent::start_at_login(&exe, true))
                    {
                        ferrite_core::log!("the server will not start at login: {e}");
                    }
                }
                // The Server's agents' `show_visual` calls are drawn by the
                // app's off-screen browser (ADR 0013).
                let renderer: std::sync::Arc<dyn ferrite_core::visual::Renderer> =
                    std::sync::Arc::new(visual::renderer::start(cx));
                // This computer's Server, or `--remote`'s machine's (ADR
                // 0026); the nav lists the others.
                let machines = cx.new(|_| {
                    machines::Machines::new(
                        machines::Reach {
                            settings_dir: settings_dir.clone(),
                            store: store_dir(),
                            executable: std::env::current_exe().unwrap_or_default(),
                            client: ferrite_core::protocol::Peer {
                                name: "ferrite".into(),
                                version: env!("CARGO_PKG_VERSION").into(),
                            },
                            renderer: Some(renderer),
                            defaults: defaults.clone(),
                            bridge: ferrite_core::machine::Bridge::ssh,
                        },
                        &settings.machines,
                        remote.as_deref(),
                    )
                });
                let core = machines
                    .update(cx, |machines, _| machines.cockpit())
                    .unwrap_or_else(|e| {
                        ferrite_core::log!("cannot read the Thread store: {e}");
                        std::process::exit(1);
                    });
                (core, Some(machines))
            };
            let (adopted, refused) = adopt(&mut core, &imports);
            for refusal in refused {
                ferrite_core::log!("{refusal}");
            }
            // A Dock launch has no directory of its own either: launchd starts
            // it in `/`, which is no Project. It stands where the newest Thread
            // works instead, so the launch project and every draft begin there.
            if dock {
                if let Err(e) = std::env::set_current_dir(dock_launch_dir(&core)) {
                    ferrite_core::log!("cannot stand in the launch directory: {e}");
                }
            }
            if demo {
                core.watch_memory(Box::new(ProcessRss::default()), spawner::RSS_LIMIT);
            }
            for thread in adopted {
                if let Err(e) = core.revive(thread) {
                    ferrite_core::log!("imported thread {thread} would not open: {e}");
                }
            }
            // The Server restores the Threads that were open; an empty
            // store starts as a draft Pane (#29).
            if core.threads().is_empty() && (demo || panes > 1) {
                demo::seed_panes(&mut core, panes, provider, demo && !load);
                if load {
                    demo::seed_load_group(&mut core);
                }
            }

            // The demo's Group (above): the fixture opens *on* it. The default
            // view is Solo (#28), which is right for a launch but wrong here —
            // the board the demo exists to draw is a Group's membership, and
            // Solo would show one Pane of it.
            let seeded_group = (demo && !parity)
                .then(|| core.groups().iter().next().map(|group| group.id))
                .flatten();

            // Who posts the system notifications (ADR 0017): the bundle's
            // identifier, and on Windows the AppUserModelID.
            cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite");
            let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
            let mut cockpit_window = None;
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        // The titlebar blends into the app: on macOS the
                        // traffic lights sit in the nav's top chrome (#22
                        // D24). Windows has no
                        // lights to keep, so hiding its titlebar hides the
                        // caption buttons too — `titlebar.rs` draws them back
                        // into the same band, which is the whole point: one
                        // band, not the system's bar above the app's.
                        titlebar: Some(TitlebarOptions {
                            title: Some("ferrite".into()),
                            appears_transparent: cfg!(target_os = "macos") || titlebar::CUSTOM,
                            traffic_light_position: Some(point(
                                px(theme::TRAFFIC_X),
                                px(theme::TRAFFIC_Y),
                            )),
                        }),
                        // Only the empty top strip moves the window;
                        // Pane heads and transcript text below it keep
                        // their own drag gestures (`titlebar::strip`).
                        app_owns_titlebar_drag: cfg!(target_os = "macos"),
                        // Below this the title, the add control and the
                        // caption group collide, and no Pane reaches L2.
                        window_min_size: Some(size(
                            px(theme::WINDOW_MIN_W),
                            px(theme::WINDOW_MIN_H),
                        )),
                        // Glass on the frame (theme rule 4): macOS blurs
                        // what is behind the window, and every region paints
                        // its own translucent fill over it; elsewhere the
                        // window is opaque grey.
                        window_background: theme::window_background(),
                        // Windows draws a DirectComposition window with no
                        // redirection bitmap: shown before gpui presents,
                        // it is an empty see-through frame. It opens hidden
                        // and is shown with its first frame (below).
                        show: !cfg!(target_os = "windows"),
                        ..Default::default()
                    },
                    |window, cx| {
                        let view = cx.new(|cx| {
                            let mut view = CockpitView::new_with_settings(
                                core,
                                provider,
                                cockpit::Preferences {
                                    settings: settings.clone(),
                                    dir: settings_dir.clone(),
                                    defaults: defaults.clone(),
                                    titler: true,
                                    cli_updates: true,
                                    ferrite_updates,
                                    whats_new: whats_new.clone(),
                                    unreadable: unreadable.clone(),
                                    keys_refused: keys_refused.clone(),
                                    board: true,
                                },
                                cx,
                            );
                            if let Some(group) = seeded_group {
                                view.enter_group(group, cx);
                            }
                            match machines {
                                Some(machines) => view.with_machines(machines),
                                None => view,
                            }
                        });
                        // The kit Root paints its theme's opaque `background`
                        // under everything; on glass the window's root paints
                        // nothing (`paint::WINDOW`), so the blur reaches the
                        // regions' own fills.
                        let content =
                            cx.new(|cx| loops_overlay::CockpitWindow::new(view, window, cx));
                        // The Cockpit the window shows when it is asked:
                        // it is built afresh on another machine.
                        let closing = content.downgrade();
                        window.on_window_should_close(cx, move |window, cx| {
                            closing
                                .update(cx, |content, _| content.cockpit().clone())
                                .map(|view| {
                                    view.update(cx, |view, cx| view.close_requested(window, cx))
                                })
                                .unwrap_or(true)
                        });
                        cockpit_window = Some(content.downgrade());
                        cx.new(|cx| {
                            kit::component::Root::new(content, window, cx)
                                .bordered(false)
                                .bg(theme::paint::WINDOW)
                        })
                    },
                )
                .unwrap();
            // ⌘Q asks first while an agent works or a Decision waits — in
            // this process; a Server's agents keep running.
            let cockpit_window: WeakEntity<loops_overlay::CockpitWindow> =
                cockpit_window.expect("the window built its Cockpit");
            cx.on_action(move |_: &Quit, cx| {
                let asked = window.update(cx, |_, window, cx| {
                    cockpit_window
                        .update(cx, |content, _| content.cockpit().clone())
                        .map(|view| view.update(cx, |view, cx| view.request_quit(window, cx)))
                });
                if !matches!(asked, Ok(Ok(()))) {
                    cx.quit();
                }
            });

            window
                .update(cx, |_, window, cx| {
                    // A hidden window is sent no frames: `open_window` has
                    // drawn the first one, and showing it presents it.
                    if cfg!(target_os = "windows") {
                        window.activate_window();
                    }
                    cx.activate(true)
                })
                .unwrap();
        });
}

/// Adopt CLI sessions started outside Ferrite. Each Thread is durable the
/// moment import returns, so it opens like any parked one. A refusal is the
/// operator's to read: the file is named and the provider's own words are
/// shown, and the run carries on without it.
fn adopt(cockpit: &mut Cockpit, paths: &[String]) -> (Vec<ThreadId>, Vec<String>) {
    let mut adopted = Vec::new();
    let mut refused = Vec::new();
    for path in paths {
        match cockpit.import(std::path::Path::new(path)) {
            Ok(thread) => adopted.push(thread),
            // Reported, not printed: the caller decides where an operator
            // reads it, and a test can read it too.
            Err(e) => refused.push(format!("cannot import {path}: {e}")),
        }
    }
    (adopted, refused)
}

/// Where a Dock launch stands, in place of the `/` launchd starts it in:
/// the directory the newest Thread works in — its Project's root, else its
/// binding's repo — skipping Threads whose directory is gone; with no
/// Thread, the newest registered Project still on disk; with none, home,
/// where a new terminal opens.
fn dock_launch_dir(cockpit: &Cockpit) -> std::path::PathBuf {
    let parked = cockpit.parked().unwrap_or_default();
    let worked = parked
        .iter()
        .rev()
        .filter_map(|thread| cockpit.peek(*thread).ok())
        .filter_map(|meta| {
            meta.project_id
                .and_then(|id| cockpit.registry().project(id))
                .map(|project| project.root.clone())
                .or_else(|| {
                    meta.workspace.map(|binding| match binding {
                        WorkspaceBinding::Main { checkout } => checkout,
                        WorkspaceBinding::Worktree { repo, .. } => repo,
                    })
                })
        })
        .find(|dir| dir.is_dir());
    let registered = || {
        cockpit
            .registry()
            .projects()
            .iter()
            .rev()
            .map(|project| project.root.clone())
            .find(|root| root.is_dir())
    };
    worked
        .or_else(registered)
        .or_else(home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("/"))
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let at = args.iter().position(|arg| arg == name)?;
    args.get(at + 1).map(String::as_str)
}

/// Marks the bindings Ferrite's key table made, apart from the ones the
/// kit (its inputs, text views, dialogs) and `rich` bind for themselves.
const KEY_TABLE: KeyBindingMetaIndex = KeyBindingMetaIndex(0);

/// Bind `table` (the defaults with the operator's keys laid over them) in
/// place of the key table bound before, and make it the table every drawn
/// chord reads; the menu bar is set again so its key equivalents follow.
///
/// gpui removes no single binding, so the keymap is rebuilt: everyone
/// else's bindings stay, in order, and the new table takes the old one's
/// place (the end, the first time), keeping same-depth precedence.
fn bind_keys(table: Vec<keymap::Binding>, cx: &mut App) {
    keymap::install(table.clone());
    let mut table = Some(load_bindings(table, cx));
    let keymap = cx.key_bindings();
    let mut rebuilt = Vec::new();
    for binding in keymap.borrow().bindings() {
        if binding.meta() == Some(KEY_TABLE) {
            rebuilt.extend(table.take().into_iter().flatten());
        } else {
            rebuilt.push(binding.clone());
        }
    }
    rebuilt.extend(table.into_iter().flatten());
    cx.clear_key_bindings();
    cx.bind_keys(rebuilt);
    cx.set_menus(app_menus());
}

/// A key table (crate::keymap), built into live bindings. The table is
/// data so both platforms' spellings stay testable; actions are rebuilt
/// from their registered names here, and the test below keeps a renamed
/// action from surviving to a launch panic.
fn load_bindings(table: Vec<keymap::Binding>, cx: &mut App) -> Vec<KeyBinding> {
    table
        .into_iter()
        .map(|(keystroke, action, context)| {
            let action = cx
                .build_action(action, None)
                .expect("the keymap names registered actions");
            let context = context.map(|context| {
                KeyBindingContextPredicate::parse(context)
                    .expect("the keymap contexts parse")
                    .into()
            });
            KeyBinding::load(
                &keystroke,
                action,
                context,
                false,
                None,
                &DummyKeyboardMapper,
            )
            .expect("the keymap keystrokes parse")
            .with_meta(KEY_TABLE)
        })
        .collect()
}

/// The application menu bar: every cockpit verb the keys already run,
/// spelled out where a person looks for it, with the platform's own edit
/// menu so the standard cut/copy/paste/select-all reach the Composer.
fn app_menus() -> Vec<Menu> {
    use cockpit::{
        CloseThread, NewGroup, NewThread, NewWorktreeThread, NextDecision, NextPane, OpenSettings,
        PreviousPane, ReopenThread, ToggleFullscreen, ToggleNav, ToggleNotifications,
    };
    vec![
        Menu {
            disabled: false,
            name: "Ferrite".into(),
            items: vec![
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit Ferrite", Quit),
            ],
        },
        Menu {
            disabled: false,
            name: "File".into(),
            items: vec![
                MenuItem::action("New Thread", NewThread),
                MenuItem::action("New Thread in a Worktree", NewWorktreeThread),
                // ⌘⇧G: the focused Thread and a new one, founding a Group.
                MenuItem::action("New Group", NewGroup),
                // ⌘G: the palette scoped to Groups.
                MenuItem::action("Open Group\u{2026}", palette::OpenGroups),
                MenuItem::separator(),
                MenuItem::action("Park Thread", CloseThread),
                MenuItem::action("Reopen Parked Thread", ReopenThread),
                MenuItem::action("Show Parked", palette::ShowParked),
            ],
        },
        Menu {
            disabled: false,
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Undo", composer::Undo, OsAction::Undo),
                MenuItem::os_action("Redo", composer::Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", composer::Cut, OsAction::Cut),
                MenuItem::os_action("Copy", composer::Copy, OsAction::Copy),
                MenuItem::os_action("Paste", composer::Paste, OsAction::Paste),
                MenuItem::os_action("Select All", composer::SelectAll, OsAction::SelectAll),
                MenuItem::separator(),
                MenuItem::action("Find\u{2026}", find::Open),
            ],
        },
        Menu {
            disabled: false,
            name: "View".into(),
            items: vec![
                MenuItem::action("Commands\u{2026}", palette::Toggle),
                MenuItem::action("Review Branch", palette::ReviewBranch),
                MenuItem::separator(),
                MenuItem::action("Toggle Sidebar", ToggleNav),
                MenuItem::action("Notifications", ToggleNotifications),
                MenuItem::action("Toggle Pane Fullscreen", ToggleFullscreen),
                MenuItem::separator(),
                MenuItem::action("Next Pane", NextPane),
                MenuItem::action("Previous Pane", PreviousPane),
                MenuItem::action("Next Decision", NextDecision),
                MenuItem::separator(),
                MenuItem::action("Keyboard Shortcuts", shortcuts::Toggle),
            ],
        },
        Menu {
            disabled: false,
            name: "Window".into(),
            items: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    // No `use super::*`: the crate root globs `gpui::*`, whose `test` macro
    // would capture the `#[test]` this macro expands to and recurse.
    use super::{
        adopt, app_menus, bind_keys, demo, dock_launch_dir, find, keep_mouse_cursor_visible,
        keymap, load_bindings, KEY_TABLE,
    };
    use ferrite_core::cockpit::Cockpit;
    use ferrite_core::store::{Provider, Store};
    use ferrite_core::workspace::WorkspaceChoice;

    /// The keymap's action names are strings (the ones with no default key
    /// too); startup rebuilds real actions from them. Without this, a renamed action would only fail at launch.
    #[gpui::test]
    fn every_table_entry_builds_for_both_platforms(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for platform in [keymap::Platform::Mac, keymap::Platform::Windows] {
                assert_eq!(
                    load_bindings(keymap::bindings(platform), cx).len(),
                    keymap::bindings(platform).len()
                );
            }
            for (action, _) in keymap::UNBOUND {
                assert!(cx.build_action(action, None).is_ok(), "{action}");
            }
        });
    }

    /// Binding the key table, at launch or on `reset keys`, replaces only
    /// the table: the kit's own keys (an input's backspace, rich text's
    /// copy) stay bound, and the table is never bound twice.
    #[gpui::test]
    fn binding_the_key_table_keeps_the_kits_keys(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::theme::init_components(cx);
            let kit = |cx: &mut gpui::App| {
                cx.key_bindings()
                    .borrow()
                    .bindings()
                    .filter(|binding| binding.meta() != Some(KEY_TABLE))
                    .count()
            };
            let ours = |cx: &mut gpui::App| {
                cx.key_bindings()
                    .borrow()
                    .bindings()
                    .filter(|binding| binding.meta() == Some(KEY_TABLE))
                    .count()
            };
            let before = kit(cx);
            // The defaults, as other tests read the installed table too.
            bind_keys(keymap::bindings(keymap::PLATFORM), cx);
            bind_keys(keymap::bindings(keymap::PLATFORM), cx);
            assert_eq!(kit(cx), before);
            assert_eq!(ours(cx), keymap::bindings(keymap::PLATFORM).len());
            assert!(cx
                .key_bindings()
                .borrow()
                .bindings()
                .any(|binding| binding.action().name() == "input::Backspace"));
        });
    }

    /// The Edit menu finds, as ⌘F does: the menu bar spells the key from
    /// the same action's binding.
    #[test]
    fn the_edit_menu_finds() {
        let edit = app_menus()
            .into_iter()
            .find(|menu| menu.name == "Edit")
            .expect("an Edit menu");
        assert!(edit.items.iter().any(|item| matches!(
            item,
            gpui::MenuItem::Action { name, action, .. }
                if name == "Find\u{2026}" && action.partial_eq(&find::Open)
        )));
    }

    #[gpui::test]
    fn typing_does_not_hide_the_mouse_cursor(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            keep_mouse_cursor_visible(cx);
            assert_eq!(cx.cursor_hide_mode(), gpui::CursorHideMode::Never);
        });
    }

    /// A Dock launch stands where the newest Thread works — launchd's `/`
    /// is no Project — else in the newest registered Project, else at home.
    #[test]
    fn a_dock_launch_stands_where_the_newest_thread_works() {
        let dir = std::env::temp_dir().join(format!("ferrite-launch-{}-dock", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = super::home_dir().expect("a home directory is set");
        let store = Store::open(dir.clone()).unwrap();
        let mut core = Cockpit::new(store, Box::new(demo::Spawn::new(false)));
        assert_eq!(dock_launch_dir(&core), home, "an empty store: home");

        let checkout = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
        core.register_project(&checkout).unwrap();
        assert_eq!(
            dock_launch_dir(&core),
            checkout,
            "a registered Project, no Thread yet: the Project"
        );
        core.open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: checkout.clone(),
            },
        )
        .unwrap();
        // A newer Thread in a directory that is gone by the next launch.
        let gone = dir.join("gone");
        std::fs::create_dir_all(&gone).unwrap();
        core.open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: gone.clone(),
            },
        )
        .unwrap();
        drop(core);
        std::fs::remove_dir_all(&gone).unwrap();
        // The next launch: every Thread parked, the newest whose directory
        // still exists the one that answers.
        let core = Cockpit::new(Store::open(dir).unwrap(), Box::new(demo::Spawn::new(false)));
        assert_eq!(dock_launch_dir(&core), checkout);
    }

    /// Leg 3: a file that is not a session file is refused in the operator's
    /// words, and the cockpit carries on without it.
    #[test]
    fn an_unimportable_file_is_refused_and_adopted_by_nobody() {
        let dir = std::env::temp_dir().join(format!(
            "ferrite-launch-{}-import-refusal",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bogus = dir.join("not-a-session.jsonl");
        std::fs::write(&bogus, "this is not a session file\n").unwrap();
        let store = Store::open(dir.join("threads")).unwrap();
        let mut core = Cockpit::new(store.clone(), Box::new(demo::Spawn::new(false)));

        let (adopted, refused) = adopt(&mut core, &[bogus.to_string_lossy().to_string()]);

        assert!(adopted.is_empty());
        // The operator is told what was refused and why, in the provider's
        // own words — not left with a launch that quietly did nothing.
        assert_eq!(refused.len(), 1);
        assert!(
            refused[0].contains("not-a-session.jsonl") && refused[0].contains("not an importable"),
            "unhelpful refusal: {}",
            refused[0]
        );
        // Nothing half-made was left in the store either.
        assert!(store.thread_ids().unwrap().is_empty());
    }
}
