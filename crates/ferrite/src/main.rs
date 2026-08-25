// Ferrite app shell: one Thread in one Pane — the walking skeleton.
mod composer;
mod line;
mod pane;
mod session;

use ferrite_core::providers::{ClaudeConfig, ClaudeSession, CodexConfig, CodexSession};
use gpui::*;

use pane::Pane;
use session::{DemoSession, Session};

actions!(ferrite, [Quit]);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let demo = args.iter().any(|arg| arg == "--demo");
    let provider = args
        .iter()
        .position(|arg| arg == "--provider")
        .and_then(|at| args.get(at + 1))
        .cloned();

    Application::new().run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("backspace", composer::Backspace, None),
            KeyBinding::new("delete", composer::Delete, None),
            KeyBinding::new("left", composer::Left, None),
            KeyBinding::new("right", composer::Right, None),
            KeyBinding::new("home", composer::Home, None),
            KeyBinding::new("end", composer::End, None),
            KeyBinding::new("cmd-v", composer::Paste, None),
            KeyBinding::new("enter", pane::Submit, None),
            // Only while a Decision holds the keyboard: elsewhere y and n are
            // just letters going into the Composer.
            KeyBinding::new("y", pane::Allow, Some("Decision")),
            KeyBinding::new("n", pane::Deny, Some("Decision")),
            KeyBinding::new("a", pane::Always, Some("Decision")),
            KeyBinding::new("escape", pane::Interrupt, None),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let session = if demo {
            Ok(Session::Demo(DemoSession::start()))
        } else {
            match provider.as_deref() {
                None | Some("claude") => spawn_claude(),
                Some("codex") => spawn_codex(),
                // A typo must not quietly run the wrong provider.
                Some(other) => Err(format!("unknown provider: {other}")),
            }
        };

        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
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
                |_, cx| cx.new(|cx| Pane::new(session, cx)),
            )
            .unwrap();

        window
            .update(cx, |pane, window, cx| {
                window.focus(&pane.composer().focus_handle(cx));
                cx.activate(true);
            })
            .unwrap();
    });
}

/// The Thread's workspace binding is the current checkout for now.
fn spawn_claude() -> Result<Session, String> {
    let config = ClaudeConfig {
        cwd: std::env::current_dir().ok(),
        ..Default::default()
    };
    ClaudeSession::spawn(config)
        .map(Session::Claude)
        .map_err(|e| e.to_string())
}

/// `--provider codex`. Decisions only reach Ferrite when the server is asked
/// to route them, so the approval policy is stated rather than inherited.
fn spawn_codex() -> Result<Session, String> {
    let config = CodexConfig {
        cwd: std::env::current_dir().ok(),
        approval_policy: Some("on-request".into()),
        ..Default::default()
    };
    CodexSession::spawn(config)
        .map(Session::Codex)
        .map_err(|e| e.to_string())
}
