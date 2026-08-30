// Ferrite: the cockpit window and the pump behind it.
mod cockpit;
mod composer;
mod fuzzy;
mod icons;
mod keymap;
mod line;
mod nav;
mod pane;
mod pointer;
mod select;
mod session;
mod theme;

use ferrite_core::cockpit::Cockpit;
use ferrite_core::store::{Provider, Store};
use ferrite_core::workspace::WorkspaceChoice;
use gpui::*;

use cockpit::CockpitView;
use session::{ProcessRss, Spawn};

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
    // How many Panes to open. The perf run uses 24.
    let panes: usize = flag(&args, "--panes")
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);

    // The icons are registered on the application, before `run`:
    // `with_assets` replaces both the asset source and the SVG renderer,
    // and `svg()` finds neither afterwards.
    Application::new().with_assets(icons::Assets).run(move |cx: &mut App| {
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

        let mut core = match Cockpit::try_new(store, Box::new(Spawn::new(demo, load))) {
            Ok(core) => core,
            Err(e) => {
                eprintln!("ferrite: cannot open the workspace registry: {e}");
                std::process::exit(1);
            }
        };
        core.watch_memory(Box::new(ProcessRss), RSS_LIMIT);
        for thread in adopted {
            if let Err(e) = core.revive(thread) {
                eprintln!("ferrite: imported thread {thread} would not open: {e}");
            }
        }
        if core.threads().is_empty() {
            if demo || panes > 1 {
                seed_panes(&mut core, panes, provider, demo);
            } else {
                // The default launch revives the newest parked Thread; an
                // empty store starts as a draft Pane (#29) — nothing
                // spawns before the operator's choice.
                cockpit::revive_latest(&mut core);
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
                    // the visible titlebar, and the traffic lights sit in
                    // its band (#22 D24). macOS only — hiding the system
                    // titlebar elsewhere would take the window controls
                    // with it.
                    titlebar: Some(TitlebarOptions {
                        title: Some("ferrite".into()),
                        appears_transparent: cfg!(target_os = "macos"),
                        traffic_light_position: Some(point(
                            px(theme::TRAFFIC_X),
                            px(theme::TRAFFIC_Y),
                        )),
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| CockpitView::new_with_provider(core, provider, cx)),
            )
            .unwrap();

        if let Some(group) = seeded_group {
            window
                .update(cx, |view, _window, cx| view.enter_group(group, cx))
                .unwrap();
        }

        window
            .update(cx, |_, _window, cx| cx.activate(true))
            .unwrap();
    });
}

/// The multi-Pane seed (`--demo`, `--panes N`): revive whatever this store
/// already parked — newest first, which `cockpit::threads_for` does — and
/// open new Threads for the room that is left.
///
/// Those new Threads alternate the two providers, starting at `first`. A
/// Cockpit opened on one provider draws the same logomark down the whole
/// nav; the design shows both marks mixed, so the seed has to deal both.
fn seed_panes(core: &mut Cockpit, panes: usize, first: Provider, demo: bool) {
    // The seeded Panes are a Group's members: with no global wall (#28) a
    // Group is the only view that shows more than one Pane. The nav's
    // selected fill is carried by the current Group too — the one holding
    // the focused Pane's Thread — so a seed of nothing but solo Threads
    // leaves that fill unpainted and every Group title at `TEXT` instead of
    // `TEXT_STRONG`. A store that already holds
    // Groups (any run after the first) is therefore revived from the first
    // Group's members before anything else; a fresh store has no Group to
    // revive from and gets one from `cockpit::seed_groups`, over the Threads
    // opened below. Panes open in ThreadId order, so the first member taken
    // here is the focused Pane's Thread.
    let members: Vec<ferrite_core::ThreadId> = core
        .groups()
        .iter()
        .next()
        .map(|group| group.members.clone())
        .unwrap_or_default();
    // ...but only into the leading Panes. The Cockpit is a census, not a
    // monoculture: a Decision waits in one Pane and a Session has closed in
    // the last, which is what paints the two coloured `.signal` lines and
    // the attention and blocked borders. A revived Thread replays its log,
    // and a log carries neither a pending Decision nor an exit — both are
    // Session state — so those Panes have to be freshly dealt. The demo
    // spawner deals its seeds in open order and a revived Thread takes
    // none, so the three Panes held back here get seeds 0, 1 and 2:
    // the Decision the interactive script stops on, a working Thread, and
    // a Session that closed.
    let revivable = panes.saturating_sub(3).max(1);
    let mut open = 0;
    for thread in members.into_iter().take(revivable) {
        match core.revive(thread) {
            Ok(()) => open += 1,
            Err(e) => eprintln!("ferrite: thread {thread} could not be revived: {e:?}"),
        }
    }
    let parked = core.parked().map(|threads| threads.len()).unwrap_or(0);
    open += cockpit::threads_for(core, revivable.saturating_sub(open).min(parked), first).len();
    let checkout = std::env::current_dir().unwrap_or_else(|_| ".".into());
    while open < panes {
        let provider = match (open % 2 == 0, first) {
            (true, first) => first,
            (false, Provider::Claude) => Provider::Codex,
            (false, Provider::Codex) => Provider::Claude,
        };
        let choice = WorkspaceChoice::Main {
            checkout: checkout.clone(),
        };
        match core.open(provider, choice) {
            Ok(_) => open += 1,
            Err(e) => {
                eprintln!("ferrite: could not open a thread: {e}");
                break;
            }
        }
    }
    // The Group tier the demo shows off, and only the demo: its titles are
    // written copy and it opens Threads of its own to fill a second Group.
    // That is fixture, exactly like the scripted transcripts around it, and
    // it may never reach a store an operator keeps work in — `--panes N`
    // alone spawns real Sessions, so it does not qualify.
    if demo {
        let seeded: Vec<ferrite_core::ThreadId> = core.threads().to_vec();
        cockpit::seed_groups(core, &seeded, first);
    }
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
