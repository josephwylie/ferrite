// Ferrite app shell. Placeholder window; the walking-skeleton Pane replaces it.
use gpui::*;

struct Shell;

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .bg(rgb(0x050505))
            .items_center()
            .justify_center()
            .text_color(rgb(0x7f8187))
            .font_family("Menlo")
            .text_size(px(12.))
            .child("ferrite")
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("ferrite".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Shell),
        )
        .unwrap();
        cx.activate(true);
    });
}
