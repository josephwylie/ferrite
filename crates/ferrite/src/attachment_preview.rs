//! Pane-owned file previews: images use a focused overlay while text files
//! open in a reader that takes its own slot on the board, beside its Pane.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use gpui::component::{
    button::{Button, ButtonVariants},
    dialog::{DialogContent, DialogHeader, DialogTitle},
    scroll::ScrollableElement,
    Icon, IconName, Sizable, Theme, ThemeStyled,
};
use gpui::{
    canvas, div, prelude::*, px, relative, rems, rgb, rgba, AnyElement, App, Bounds, Div,
    FocusHandle, IntoElement, Pixels, Window,
};

use crate::theme;

#[derive(Clone)]
pub struct Document {
    pub path: PathBuf,
    pub title: String,
    pub source: String,
}

#[derive(Default)]
struct State {
    image: Option<(PathBuf, String)>,
    document: Option<Document>,
    return_focus: Option<FocusHandle>,
}

#[derive(Clone)]
// Native Markdown callbacks require Send + Sync captures; the Pane still owns this slot.
pub struct Preview {
    state: Arc<Mutex<State>>,
    bounds: Arc<Mutex<Bounds<Pixels>>>,
    focus: FocusHandle,
    /// Tracked by the reader slot, so the cockpit can tell a selection in
    /// the reader from a click that should hand focus back to the Pane.
    reader_focus: FocusHandle,
}

impl Preview {
    pub fn new(cx: &mut App) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            bounds: Arc::new(Mutex::new(Bounds::default())),
            focus: cx.focus_handle(),
            reader_focus: cx.focus_handle(),
        }
    }

    /// Whether text inside the open reader holds focus — Markdown, code or
    /// plain text being selected in. The slot itself taking focus on a click
    /// is not text, and does not count.
    pub fn reader_text_focused(&self, window: &Window, cx: &App) -> bool {
        self.reader_focus.contains_focused(window, cx) && !self.reader_focus.is_focused(window)
    }

    pub fn focus_target(&self) -> Option<FocusHandle> {
        self.state
            .lock()
            .unwrap()
            .image
            .as_ref()
            .map(|_| self.focus.clone())
    }

    pub fn open(&self, path: PathBuf, title: String, window: &mut Window, cx: &mut App) {
        let mut state = self.state.lock().unwrap();
        if state.image.is_none() {
            state.return_focus = window.focused(cx);
        }
        // Repeated activation replaces one slot; it can never stack dialogs.
        state.image = Some((path, title));
        drop(state);
        self.focus.focus(window, cx);
        window.refresh();
    }

    /// Open a UTF-8 text file in the built-in reader. `false` means the file
    /// is binary, so callers that support it may fall back to the OS.
    pub fn open_text_document(
        &self,
        path: PathBuf,
        title: String,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        use gpui::component::{notification::Notification, WindowExt as _};
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Could not open {}: {error}", path.display())),
                    cx,
                );
                return true;
            }
        };
        let Ok(source) = String::from_utf8(bytes) else {
            return false;
        };
        self.state.lock().unwrap().document = Some(Document {
            path,
            title,
            source,
        });
        window.refresh();
        true
    }

    pub fn open_document(&self, path: PathBuf, title: String, window: &mut Window, cx: &mut App) {
        use gpui::component::{notification::Notification, WindowExt as _};
        if !self.open_text_document(path.clone(), title, window, cx) {
            window.push_notification(
                Notification::error(format!(
                    "Could not preview {} because it is not a text file",
                    path.display()
                )),
                cx,
            );
        }
    }

    pub fn document(&self) -> Option<Document> {
        self.state.lock().unwrap().document.clone()
    }

    fn close(&self, window: &mut Window, cx: &mut App) {
        let mut state = self.state.lock().unwrap();
        state.image = None;
        let focus = state.return_focus.take();
        drop(state);
        if let Some(focus) = focus {
            focus.focus(window, cx);
        }
        window.refresh();
    }

    pub fn close_document(&self, window: &mut Window) {
        self.state.lock().unwrap().document = None;
        window.refresh();
    }

    /// The Pane with its image layer: the overlay covers exactly the Pane's
    /// own bounds. The text reader is not mounted here — it is a slot of
    /// its own on the board (`reader`), laid out like any other Pane.
    pub fn mount(&self, pane: Div) -> Div {
        let preview = self.clone();
        pane.child(
            canvas(
                move |bounds, window, cx| {
                    if {
                        let mut prior = preview.bounds.lock().unwrap();
                        let changed = *prior != bounds;
                        *prior = bounds;
                        changed
                    } && preview.state.lock().unwrap().image.is_some()
                    {
                        // refresh() is ignored during prepaint. Schedule the
                        // new pane geometry after this frame completes.
                        window.defer(cx, |window, _| window.refresh());
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        .child(PreviewLayer(self.clone()))
    }

    /// The open document as a board slot: a Pane-shaped shell whose head
    /// is handed to `head` (the cockpit wires the drag that moves the slot)
    /// and whose body is the rendered `body`. None while no document is open.
    pub fn reader(&self, body: AnyElement, head: impl FnOnce(Div) -> AnyElement) -> Option<Div> {
        let document = self.document()?;
        let markdown = document.is_markdown();
        let kind = document.kind();
        let document_content = if markdown {
            div()
                .id("markdown-reader-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(
                    div()
                        .w_full()
                        .max_w(rems(52.))
                        .mx_auto()
                        .px(px(theme::PANE_PAD_X))
                        .pt(px(theme::BODY_PAD_T))
                        .pb(px(theme::BODY_PAD_B))
                        .child(body),
                )
                .into_any_element()
        } else {
            div()
                .id("markdown-reader-scroll")
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .p(px(theme::PANE_PAD_X))
                .child(div().size_full().child(body))
                .into_any_element()
        };
        let close = self.clone();
        let head_band = div()
            .flex()
            .items_center()
            .h(px(theme::PANE_HEAD_H))
            .flex_shrink_0()
            .gap_2()
            .px(px(theme::PANE_PAD_X))
            .bg(rgb(theme::PANE_HEAD))
            .rounded_t(px(theme::R_SURFACE - 1.))
            .border_b_1()
            .border_color(rgba(theme::PANE_HEAD_EDGE))
            .child(
                Icon::new(IconName::FileText)
                    .size(px(theme::ROW_ICON))
                    .text_color(rgb(theme::TEXT_2)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(theme::FONT_UI)
                    .text_size(px(theme::FS_MD))
                    .text_color(rgb(theme::TEXT_STRONG))
                    .child(document.title),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .rounded(px(theme::R_CHIP))
                    .bg(rgb(theme::RAISED))
                    .px(px(theme::CHIP_PAD_X))
                    .py(px(theme::CHIP_PAD_Y))
                    .text_size(px(theme::FS_MONO))
                    .text_color(rgb(theme::TEXT_MUTED))
                    .child(kind),
            )
            .child(
                div()
                    .debug_selector(|| "close-markdown-reader".into())
                    // A press here closes; it must not also pick the slot
                    // up as a drag.
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        crate::components::button("close-markdown-reader")
                            .w(px(theme::ICON_BUTTON))
                            .h(px(theme::ICON_BUTTON))
                            .p_0()
                            .tooltip("Close document reader")
                            .child(
                                Icon::new(IconName::Close)
                                    .size(px(theme::ICON_BUTTON_GLYPH))
                                    .text_color(rgb(theme::TEXT_MUTED)),
                            )
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                close.close_document(window);
                            }),
                    ),
            );
        Some(
            div()
                .debug_selector(|| "markdown-reader".into())
                .track_focus(&self.reader_focus)
                .relative()
                .flex()
                .flex_col()
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .rounded(px(theme::R_SURFACE))
                .border_1()
                .border_color(rgba(theme::TRANSPARENT))
                .bg(rgb(theme::PANE))
                .font_family(theme::FONT_MONO)
                .child(head(head_band))
                .child(document_content),
        )
    }
}

/// The OS opens the original file in its image viewer, where it can be
/// inspected at full resolution. Canonicalize before URL encoding so relative
/// paths, symlinks and Windows drive paths all use the same file-link route.
fn open_original(path: &Path, window: &mut Window, cx: &mut App) {
    use gpui::component::{notification::Notification, WindowExt};
    match std::fs::canonicalize(path) {
        Ok(path) => crate::file_links::FileLink {
            path,
            location: None,
        }
        .open(window, cx),
        Err(error) => window.push_notification(
            Notification::error(format!("Could not open {}: {error}", path.display())),
            cx,
        ),
    }
}

impl Document {
    /// The reader's type chip: the language for a file the lexer knows, else
    /// the extension as a file card shows it, else just `FILE`.
    pub fn kind(&self) -> String {
        if self.is_markdown() {
            return "MARKDOWN".into();
        }
        if let Some(language) = ferrite_core::transcript::language_for_path(&self.path) {
            return match language {
                "cpp" => "C++".into(),
                language => language.to_ascii_uppercase(),
            };
        }
        match self
            .path
            .extension()
            .and_then(|extension| extension.to_str())
        {
            Some(extension) if !extension.is_empty() && extension.len() <= 8 => {
                extension.to_ascii_uppercase()
            }
            _ => "FILE".into(),
        }
    }

    pub fn is_markdown(&self) -> bool {
        self.path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
            })
    }
}

#[derive(IntoElement)]
struct PreviewLayer(Preview);

impl RenderOnce for PreviewLayer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let preview = self.0;
        let image = preview.state.lock().unwrap().image.clone();
        let Some((path, title)) = image else {
            return div().absolute().into_any_element();
        };
        let bounds = *preview.bounds.lock().unwrap();
        let close_button = preview.clone();
        let close_dialog = preview.clone();
        let original = path.clone();
        let content = div()
            .debug_selector(|| "attachment-preview-content".into())
            .w(relative(0.9))
            .h(relative(0.85))
            .max_w(rems(48.))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                DialogContent::new()
                    .size_full()
                    .popover_style(cx)
                    .p_3()
                    .gap_2()
                    .child(
                        DialogHeader::new()
                            .flex_row()
                            .items_center()
                            .flex_shrink_0()
                            .child(
                                DialogTitle::new()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(title),
                            )
                            .child(
                                Button::new("open-original-attachment")
                                    .ghost()
                                    .xsmall()
                                    .flex_shrink_0()
                                    .label("Open Original")
                                    .accessibility_label("Open original image in the default app")
                                    .tooltip("Open full-size image in the default app")
                                    .debug_selector(|| "open-original-attachment".into())
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        open_original(&original, window, cx);
                                    }),
                            )
                            .child(
                                Button::new("close-attachment-preview")
                                    .ghost()
                                    .xsmall()
                                    .flex_shrink_0()
                                    .icon(IconName::Close)
                                    .accessibility_label("Close image preview")
                                    .tooltip("Close image preview")
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        close_button.close(window, cx);
                                    }),
                            ),
                    )
                    .child(
                        div().relative().flex_1().min_h_0().w_full().child(
                            gpui::img(path)
                                .absolute()
                                .inset_0()
                                .size_full()
                                .object_fit(gpui::ObjectFit::Contain),
                        ),
                    ),
            );
        gpui::base::Dialog::new(cx)
            .focus_handle(preview.focus.clone())
            .left(bounds.origin.x)
            .top(bounds.origin.y)
            .w(bounds.size.width)
            .h(bounds.size.height)
            .backdrop(div().size_full().bg(Theme::global(cx).overlay))
            .popup(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(content),
            )
            .request_close(move |_, window, cx| close_dialog.close(window, cx))
            .into_any_element()
    }
}
