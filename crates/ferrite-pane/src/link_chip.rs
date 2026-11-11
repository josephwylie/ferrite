//! Inline chips in prose (T3's `ContextChip`, ADR 0038): a link to a file or
//! to a Thread is a 1em icon and a medium-weight name in the kind's accent,
//! no ground, underlined only under the pointer. The native Markdown flow
//! reserves the size the renderer returns and wraps the chip atomically, so
//! the name is measured in the face and size it is drawn in (the prose's).
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, App, Div, Pixels, SharedString, Size, Window};

use ferrite_ui::{icons, theme};

/// What a chip says: its icon and accent, the name, and a `:line` that never
/// gives way to an ellipsis.
pub(crate) struct Chip<'a> {
    pub icon: &'static str,
    pub ink: u32,
    pub name: &'a str,
    pub location: &'a str,
}

impl Chip<'_> {
    fn weighted(window: &Window) -> gpui::TextStyle {
        let mut face = window.text_style();
        face.font_family = theme::FONT_UI.into();
        face.font_weight = theme::W_LABEL;
        face.font_style = gpui::FontStyle::Normal;
        face
    }

    fn gap(size: Pixels) -> Pixels {
        size * theme::TOKEN_ICON_GAP_EM
    }

    /// The chip's width: the icon, its gap, the name and the location each
    /// shaped whole and rounded up with 1px to spare, so a name that fits is
    /// never ellipsized. Clamped to `INLINE_FILE_MIN_W`…`INLINE_FILE_MAX_W`.
    pub(crate) fn width(&self, size: Pixels, window: &mut Window) -> Pixels {
        let face = Self::weighted(window);
        let shaped = |text: &str| {
            if text.is_empty() {
                return px(0.);
            }
            window
                .text_system()
                .shape_line(
                    SharedString::from(text.to_owned()),
                    size,
                    &[face.to_run(text.len())],
                    None,
                )
                .width()
                .ceil()
        };
        let text = shaped(self.name) + shaped(self.location) + px(1.);
        (size + Self::gap(size) + text)
            .ceil()
            .clamp(px(theme::INLINE_FILE_MIN_W), px(theme::INLINE_FILE_MAX_W))
    }

    /// The chip drawn `width` wide on a line `line` tall. It is not
    /// interactive: its owner wraps it in the press.
    pub(crate) fn face(&self, size: Pixels, width: Pixels, line: Pixels) -> Div {
        div()
            .flex()
            .items_center()
            .gap(Self::gap(size))
            .w(width)
            .h(line)
            .min_w_0()
            .font_family(theme::FONT_UI)
            .font_weight(theme::W_LABEL)
            .text_color(rgb(self.ink))
            .whitespace_nowrap()
            .not_italic()
            .hover(|style| style.underline())
            .child(icons::icon(self.icon, f32::from(size), self.ink))
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(self.name.to_owned())),
                    )
                    .when(!self.location.is_empty(), |name| {
                        name.child(
                            div()
                                .flex_none()
                                .text_color(rgb(theme::INK_SUBTLE))
                                .child(SharedString::from(self.location.to_owned())),
                        )
                    }),
            )
    }
}

/// A Thread linked in prose, `[title](ferrite-thread:12)`: the thread's
/// icon and title as a chip; a press asks the transcript's owner to open it
/// (`TranscriptEvent::OpenThread`). T3's chip leads with the project's
/// favicon; Ferrite's links carry only the Thread, so it leads with the
/// Thread's own glyph.
pub(crate) fn thread_chip(
    thread: ferrite_core::ThreadId,
    title: &str,
    scope: Option<SharedString>,
    window: &mut Window,
) -> (Size<Pixels>, AnyElement) {
    let size = window.text_style().font_size.to_pixels(window.rem_size());
    let line = window.line_height();
    let chip = Chip {
        icon: icons::MESSAGES_SQUARE,
        ink: theme::THREAD_INK,
        name: title.trim(),
        location: "",
    };
    let width = chip.width(size, window);
    let selector = format!("thread-link-{}", thread.get());
    let element = div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .relative()
        .w(width)
        .h(line)
        .cursor_pointer()
        .tooltip(ferrite_ui::menu::tooltip("Open thread"))
        .on_click(move |_, window: &mut Window, cx: &mut App| {
            gpui::base::TextSelection::end(window, cx);
            cx.stop_propagation();
            if let Some(scope) = &scope {
                crate::file_links::open_thread(scope, thread, window, cx);
            }
        })
        .child(chip.face(size, width, line));
    (gpui::size(width, line), element.into_any_element())
}
