// Ferrite: the cockpit window and the pump behind it.
mod cockpit;
mod composer;
mod keymap;
mod line;
mod nav;
mod pane;
mod session;
mod theme;

use ferrite_core::cockpit::Cockpit;
use ferrite_core::store::{Provider, Store};
use gpui::*;

use cockpit::CockpitView;
use session::{ProcessRss, Spawn};

actions!(ferrite, [Quit]);

/// One Session may hold this much before the watchdog replaces it. Generous:
/// a busy agent legitimately grows, and a restart costs the operator context.
const RSS_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let load = args.iter().any(|arg| arg == "--load");
    let demo = load || args.iter().any(|arg| arg == "--demo");
    let provider = match flag(&args, "--provider") {
        None | Some("claude") => Provider::Claude,
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
    // How many Panes to open. The wall's own number is 24, which is what the
    // perf run uses.
    let panes: usize = flag(&args, "--panes")
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);

    Application::new().run(move |cx: &mut App| {
        let bindings = load_bindings(keymap::PLATFORM, cx);
        cx.bind_keys(bindings);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let store = match Store::open(store_dir()) {
            Ok(store) => store,
            Err(e) => {
                eprintln!("ferrite: cannot open the Thread store: {e}");
                std::process::exit(1);
            }
        };
        let (adopted, refused) = cockpit::adopt(&store, &imports);
        for refusal in refused {
            eprintln!("ferrite: {refusal}");
        }

        let mut core = Cockpit::new(store, Box::new(Spawn { demo, load }));
        core.watch_memory(Box::new(ProcessRss), RSS_LIMIT);
        for thread in adopted {
            if let Err(e) = core.revive(thread) {
                eprintln!("ferrite: imported thread {thread} would not open: {e}");
            }
        }
        if core.threads().is_empty() {
            cockpit::threads_for(&mut core, panes.max(1), provider);
        }

        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("ferrite".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| CockpitView::new(core, cx)),
            )
            .unwrap();

        window
            .update(cx, |_, _window, cx| cx.activate(true))
            .unwrap();
    });
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
    use super::{keymap, load_bindings};

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
}
