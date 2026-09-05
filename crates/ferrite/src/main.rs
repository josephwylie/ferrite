// Ferrite: the cockpit window and the pump behind it.
mod cockpit;
mod components;
mod composer;
mod demo;
mod facts;
mod fuzzy;
mod icons;
mod keymap;
mod line;
mod menu;
mod nav;
mod pane;
mod pointer;
mod prefs;
mod select;
mod session;
mod shell;
mod theme;
mod titlebar;

use ferrite_core::cockpit::Cockpit;
use ferrite_core::settings::Settings;
use ferrite_core::store::{Provider, Store};
use ferrite_core::workspace::WorkspaceBinding;
use ferrite_core::ThreadId;
use gpui::*;

use cockpit::CockpitView;
use session::{ProcessRss, SessionDefaults};

actions!(ferrite, [Quit]);

/// One Session may hold this much before the watchdog replaces it. Generous:
/// a busy agent legitimately grows, and a restart costs the operator context.
const RSS_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

/// The four static JetBrains Mono instances, compiled in. gpui has no
/// variation-axis support, so the prototype's one variable face cannot
/// serve: each weight is its own file. All four share the typographic
/// family `theme::FONT_MONO`, and CoreText resolves the right face from
/// `.font_weight(..)` — see that constant's own doc for the measured
/// FontId table, and never reach a weight by family name.
static JBM_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
static JBM_MEDIUM: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf");
static JBM_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf");
static JBM_BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf");

fn main() {
    // Before the args, the store, or any spawn: a Dock launch has no PATH
    // worth the name until the login shell is asked (crate::shell).
    let dock = shell::adopt_login_environment();
    let args: Vec<String> = std::env::args().collect();
    let load = args.iter().any(|arg| arg == "--load");
    let demo = load || args.iter().any(|arg| arg == "--demo");
    // The operator's settings live beside the store: `~/.ferrite`.
    let settings_dir = settings_dir();
    let settings = Settings::load(&settings_dir);
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
    // How many Panes to open. The perf run uses 24.
    let panes: usize = flag(&args, "--panes")
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);

    // The icons are registered on the application, before `run`:
    // `with_assets` replaces both the asset source and the SVG renderer,
    // and `svg()` finds neither afterwards.
    Application::new()
        .with_assets(icons::Assets)
        .run(move |cx: &mut App| {
            theme::init_components(cx);
            // First, before anything can lay out text in it: the bundled mono
            // face. `add_fonts` returns a Result and a discarded one fails
            // silently — you get the system font and no explanation.
            cx.text_system()
                .add_fonts(vec![
                    std::borrow::Cow::Borrowed(JBM_REGULAR),
                    std::borrow::Cow::Borrowed(JBM_MEDIUM),
                    std::borrow::Cow::Borrowed(JBM_SEMIBOLD),
                    std::borrow::Cow::Borrowed(JBM_BOLD),
                ])
                .expect("the bundled JetBrains Mono faces load");

            let bindings = load_bindings(keymap::PLATFORM, cx);
            cx.bind_keys(bindings);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.set_menus(app_menus());

            let store = match Store::open(store_dir()) {
                Ok(store) => store,
                Err(e) => {
                    eprintln!("ferrite: cannot open the Thread store: {e}");
                    std::process::exit(1);
                }
            };
            let (adopted, refused) = adopt(&store, &imports);
            for refusal in refused {
                eprintln!("ferrite: {refusal}");
            }

            // The fixture adapter serves `--demo` and `--load`; every other
            // launch spawns the real provider CLIs.
            let defaults = std::sync::Arc::new(std::sync::Mutex::new(
                SessionDefaults::from_settings(&settings),
            ));
            let spawner: Box<dyn ferrite_core::cockpit::Spawner> = if demo {
                Box::new(demo::Spawn::new(load))
            } else {
                Box::new(session::Spawn::new(defaults.clone()))
            };
            let mut core = match Cockpit::try_new(store, spawner) {
                Ok(core) => core,
                Err(e) => {
                    eprintln!("ferrite: cannot open the workspace registry: {e}");
                    std::process::exit(1);
                }
            };
            // A Dock launch has no directory of its own either: launchd starts
            // it in `/`, which is no Project. It stands where the newest Thread
            // works instead, so the launch project and every draft begin there.
            if dock {
                if let Err(e) = std::env::set_current_dir(dock_launch_dir(&core)) {
                    eprintln!("ferrite: cannot stand in the launch directory: {e}");
                }
            }
            core.watch_memory(Box::new(ProcessRss), RSS_LIMIT);
            for thread in adopted {
                if let Err(e) = core.revive(thread) {
                    eprintln!("ferrite: imported thread {thread} would not open: {e}");
                }
            }
            if core.threads().is_empty() {
                if demo || panes > 1 {
                    demo::seed_panes(&mut core, panes, provider, demo);
                } else {
                    // The default launch revives the newest parked Thread; an
                    // empty store starts as a draft Pane (#29) — nothing
                    // spawns before the operator's choice.
                    revive_latest(&mut core);
                }
            }

            // The demo's Group (above): the fixture opens *on* it. The default
            // view is Solo (#28), which is right for a launch but wrong here —
            // the board the demo exists to draw is a Group's membership, and
            // Solo would show one Pane of it.
            let seeded_group = demo
                .then(|| core.groups().iter().next().map(|group| group.id))
                .flatten();

            let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        // The titlebar band blends into the app: the strip is
                        // the visible titlebar, and on macOS the traffic
                        // lights sit in its band (#22 D24). Windows has no
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
                                },
                                cx,
                            );
                            if let Some(group) = seeded_group {
                                view.enter_group(group, cx);
                            }
                            view
                        });
                        cx.new(|cx| gpui_component::Root::new(view, window, cx))
                    },
                )
                .unwrap();

            window
                .update(cx, |_, _window, cx| cx.activate(true))
                .unwrap();
        });
}

/// Adopt CLI sessions started outside Ferrite, before the Cockpit takes the
/// store. Each Thread is durable the moment import returns, so it opens like
/// any parked one. A refusal is the operator's to read: the file is named and
/// the provider's own words are shown, and the run carries on without it.
fn adopt(store: &Store, paths: &[String]) -> (Vec<ThreadId>, Vec<String>) {
    let mut adopted = Vec::new();
    let mut refused = Vec::new();
    for path in paths {
        match ferrite_core::import::import(store, std::path::Path::new(path)) {
            Ok(thread) => adopted.push(thread),
            // Reported, not printed: the caller decides where an operator
            // reads it, and a test can read it too.
            Err(e) => refused.push(format!("cannot import {path}: {e}")),
        }
    }
    (adopted, refused)
}

/// Revive the newest parked Thread for launch, if the store holds any. An
/// empty store starts as a draft Pane instead (#29): nothing spawns before
/// the operator's choice.
fn revive_latest(cockpit: &mut Cockpit) {
    let Some(thread) = cockpit.parked().unwrap_or_default().last().copied() else {
        return;
    };
    if let Err(e) = cockpit.revive(thread) {
        eprintln!("ferrite: thread {thread} could not be revived: {e:?}");
    }
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
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from))
        .unwrap_or_else(|| std::path::PathBuf::from("/"))
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let at = args.iter().position(|arg| arg == name)?;
    args.get(at + 1).map(String::as_str)
}

/// Where Threads live between runs.
/// The platform key table (crate::keymap), built into live bindings. The
/// table is data so both platforms' spellings stay testable; actions are
/// rebuilt from their registered names here, and the test below keeps a
/// renamed action from surviving to a launch panic.
fn load_bindings(platform: keymap::Platform, cx: &mut App) -> Vec<KeyBinding> {
    keymap::bindings(platform)
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
        })
        .collect()
}

/// The application menu bar: every cockpit verb the keys already run,
/// spelled out where a person looks for it, with the platform's own edit
/// menu so the standard cut/copy/paste/select-all reach the Composer.
fn app_menus() -> Vec<Menu> {
    use cockpit::{
        CloseThread, NewThread, NewWorktreeThread, NextDecision, NextPane, OpenSettings,
        PreviousPane, ReopenThread, ToggleFullscreen, ToggleNav,
    };
    vec![
        Menu {
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
            name: "File".into(),
            items: vec![
                MenuItem::action("New Thread", NewThread),
                MenuItem::action("New Thread in a Worktree", NewWorktreeThread),
                MenuItem::separator(),
                MenuItem::action("Close Pane", CloseThread),
                MenuItem::action("Reopen Parked Thread", ReopenThread),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Undo", composer::Undo, OsAction::Undo),
                MenuItem::os_action("Redo", composer::Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", composer::Cut, OsAction::Cut),
                MenuItem::os_action("Copy", composer::Copy, OsAction::Copy),
                MenuItem::os_action("Paste", composer::Paste, OsAction::Paste),
                MenuItem::os_action("Select All", composer::SelectAll, OsAction::SelectAll),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Toggle Sidebar", ToggleNav),
                MenuItem::action("Toggle Pane Fullscreen", ToggleFullscreen),
                MenuItem::separator(),
                MenuItem::action("Next Pane", NextPane),
                MenuItem::action("Previous Pane", PreviousPane),
                MenuItem::action("Next Decision", NextDecision),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![],
        },
    ]
}

/// Where settings live: the store's parent (`~/.ferrite`), or beside an
/// overridden store.
fn settings_dir() -> std::path::PathBuf {
    let store = store_dir();
    store
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or(store)
}

fn store_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("FERRITE_STORE") {
        return dir.into();
    }
    // Windows spells the home directory USERPROFILE, not HOME.
    let base = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(base)
        .join(".ferrite")
        .join("threads")
}

#[cfg(test)]
mod tests {
    // No `use super::*`: the crate root globs `gpui::*`, whose `test` macro
    // would capture the `#[test]` this macro expands to and recurse.
    use super::{adopt, demo, dock_launch_dir, keymap, load_bindings};
    use ferrite_core::cockpit::Cockpit;
    use ferrite_core::store::{Provider, Store};
    use ferrite_core::workspace::WorkspaceChoice;

    /// The keymap's action names are strings; startup rebuilds real actions
    /// from them. Without this, a renamed action would only fail at launch.
    #[gpui::test]
    fn every_table_entry_builds_for_both_platforms(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for platform in [keymap::Platform::Mac, keymap::Platform::Windows] {
                assert_eq!(
                    load_bindings(platform, cx).len(),
                    keymap::bindings(platform).len()
                );
            }
        });
    }

    /// A Dock launch stands where the newest Thread works — launchd's `/`
    /// is no Project — else in the newest registered Project, else at home.
    #[test]
    fn a_dock_launch_stands_where_the_newest_thread_works() {
        let dir = std::env::temp_dir().join(format!("ferrite-launch-{}-dock", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME is set"));
        let store = Store::open(dir.clone()).unwrap();
        let mut core = Cockpit::new(store, Box::new(demo::Spawn::new(false)));
        assert_eq!(dock_launch_dir(&core), home, "an empty store: home");

        let checkout = std::env::current_dir().unwrap();
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

        let (adopted, refused) = adopt(&store, &[bogus.to_string_lossy().to_string()]);

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
