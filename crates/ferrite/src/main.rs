// Ferrite app shell: one Thread in one Pane — the walking skeleton.
mod composer;
mod line;
mod pane;
mod session;
mod transcript;

use ferrite_core::providers::{ClaudeConfig, ClaudeSession};
use gpui::*;

use pane::Pane;
use session::{DemoSession, Session};

actions!(ferrite, [Quit]);

fn main() {
    let demo = std::env::args().any(|arg| arg == "--demo");

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
            KeyBinding::new("escape", pane::Interrupt, None),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let session = if demo {
            Ok(Session::Demo(DemoSession::start()))
        } else {
            spawn_live()
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
fn spawn_live() -> Result<Session, String> {
    let config = ClaudeConfig {
        cwd: std::env::current_dir().ok(),
        ..Default::default()
    };
    ClaudeSession::spawn(config)
        .map(Session::Live)
        .map_err(|e| e.to_string())
}
