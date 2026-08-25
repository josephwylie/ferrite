// Ferrite: the cockpit window and the pump behind it.
mod cockpit;
mod composer;
mod line;
mod pane;
mod session;

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
        cx.bind_keys([
            KeyBinding::new("backspace", composer::Backspace, None),
            KeyBinding::new("delete", composer::Delete, None),
            KeyBinding::new("left", composer::Left, None),
            KeyBinding::new("right", composer::Right, None),
            KeyBinding::new("home", composer::Home, None),
            KeyBinding::new("end", composer::End, None),
            KeyBinding::new("cmd-v", composer::Paste, None),
            KeyBinding::new("enter", cockpit::Submit, None),
            KeyBinding::new("escape", cockpit::Interrupt, None),
            // Only while a Decision holds the keyboard: elsewhere these are
            // just letters going into the Composer.
            KeyBinding::new("y", cockpit::Allow, Some("Decision")),
            KeyBinding::new("n", cockpit::Deny, Some("Decision")),
            KeyBinding::new("a", cockpit::Always, Some("Decision")),
            // At wall range no Pane holds a Composer, so the same keys answer
            // whichever Thread is flagged without focusing it first.
            KeyBinding::new("y", cockpit::Allow, Some("Wall")),
            KeyBinding::new("n", cockpit::Deny, Some("Wall")),
            KeyBinding::new("a", cockpit::Always, Some("Wall")),
            // The cockpit: walk the grid, and jump to whoever needs answering.
            KeyBinding::new("cmd-]", cockpit::NextPane, None),
            KeyBinding::new("cmd-[", cockpit::PreviousPane, None),
            KeyBinding::new("cmd-d", cockpit::NextDecision, None),
            KeyBinding::new("cmd-n", cockpit::NewThread, None),
            // Shift: the same new Thread, in its own worktree instead of the
            // checkout the operator is sitting in.
            KeyBinding::new("cmd-shift-n", cockpit::NewWorktreeThread, None),
            // Close parks the Thread; it is still there, and reopening revives it.
            KeyBinding::new("cmd-w", cockpit::CloseThread, None),
            // And back again: the most recently parked Thread, revived.
            KeyBinding::new("cmd-o", cockpit::ReopenThread, None),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
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
fn store_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("FERRITE_STORE") {
        return dir.into();
    }
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(base).join(".ferrite/threads")
}
