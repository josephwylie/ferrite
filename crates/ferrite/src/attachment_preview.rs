//! Pane-owned file previews: images use a focused overlay while text files
//! open in a reader that takes its own slot on the board, beside its Pane.
//! The kit owns dialog focus and dismissal; this module supplies the owning
//! Pane's bounds, which place and size the image sheet, while the scrim
//! covers the window like every modal's.

use crate::components::Tip as _;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use gpui::component::scroll::ScrollableElement;
use gpui::{
    canvas, div, prelude::*, px, rems, rgb, rgba, AnyElement, App, Bounds, Div, FocusHandle,
    IntoElement, Pixels, Window,
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
        // The reader's head is a Group head's twin (rule 2.4.6): one
        // `PANE_HEAD_H` line on the Pane's own ground, closed by a
        // `HAIRLINE` rule — a file mark in the glyph column, the title in
        // `FS_UI` `W_LABEL` `TEXT_STRONG`, its kind as a quiet meta word.
        let head_band = div()
            .flex()
            .items_center()
            .h(px(theme::PANE_HEAD_H))
            .flex_shrink_0()
            .gap(px(theme::SPACE_2))
            .pl(px(theme::PANE_PAD_X))
            .pr(px(theme::SPACE_1_5))
            .border_b_1()
            .border_color(rgba(theme::HAIRLINE))
            .child(crate::icons::icon(
                crate::icons::FILE,
                theme::ROW_ICON,
                theme::TEXT_MUTED,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(theme::FONT_UI)
                    .text_size(px(theme::FS_UI))
                    .line_height(px(theme::LH_UI))
                    .font_weight(theme::W_LABEL)
                    .text_color(rgb(theme::TEXT_STRONG))
                    .child(document.title),
            )
            .child(
                crate::components::text_meta()
                    .flex_shrink_0()
                    .child(kind.to_lowercase()),
            )
            .child(
                div()
                    .debug_selector(|| "close-markdown-reader".into())
                    // A press here closes; it must not also pick the slot
                    // up as a drag.
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        crate::components::button("close-markdown-reader")
                            // Reachable from the keyboard like the file
                            // card that opened it.
                            .tab_stop(true)
                            .w(px(theme::ICON_BUTTON))
                            .h(px(theme::ICON_BUTTON))
                            .p_0()
                            .tooltip("Close document reader")
                            .child(crate::icons::icon(
                                crate::icons::CLOSE,
                                theme::ICON_BUTTON_GLYPH,
                                theme::TEXT_MUTED,
                            ))
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
                .rounded(px(theme::R_PANE))
                .border_1()
                .border_color(rgba(theme::HAIRLINE))
                .bg(rgb(theme::PANE))
                .font_family(theme::FONT_UI)
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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let preview = self.0;
        let image = preview.state.lock().unwrap().image.clone();
        let Some((path, title)) = image else {
            return div().absolute().into_any_element();
        };
        let bounds = *preview.bounds.lock().unwrap();
        let close_button = preview.clone();
        let close_dialog = preview.clone();
        let original = path.clone();
        // The image's own proportions size the sheet: decoded once through
        // the same cache the `img` element reads, so it costs nothing twice.
        let natural = window
            .use_asset::<gpui::ImgResourceLoader>(&gpui::Resource::from(path.clone()), cx)
            .and_then(Result::ok)
            .map(|image| {
                let size = image.size(0);
                (size.width.0 as f32, size.height.0 as f32)
            });
        let (width, height) = sheet_size(
            natural,
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        );
        // The sheet recipe (`prefs::sheet`): `RAISED`, the strong hairline,
        // `R_PANE`, the float shadow; the 48px head with its title and the
        // one close control over a hairline; then the image itself on the
        // sheet, `MODAL_PAD` in from every edge. No well: nothing in a
        // sheet is darker than the sheet.
        let open = crate::components::form_button("open-original-attachment", cx)
            .flex_shrink_0()
            .h(px(crate::theme::CONTROL_H))
            .px(px(crate::theme::CONTROL_PAD_X))
            .child(crate::components::form_label(
                "Open original",
                crate::theme::TEXT_2,
            ))
            .accessibility_label("Open original image in the default app")
            .tip("Open full-size image in the default app")
            .debug_selector(|| "open-original-attachment".into())
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                open_original(&original, window, cx);
            });
        let close =
            crate::prefs::sheet_close("close-attachment-preview", "Close image preview", cx)
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    close_button.close(window, cx);
                });
        let head =
            crate::prefs::sheet_head(title, div().flex().items_center().child(open).child(close));
        let sheet = crate::prefs::sheet(width, height)
            .debug_selector(|| "attachment-preview-content".into())
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(head)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .p(px(crate::theme::MODAL_PAD))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .rounded(px(crate::theme::R_CHIP))
                            .overflow_hidden()
                            .child(
                                gpui::img(path)
                                    .absolute()
                                    .inset_0()
                                    .size_full()
                                    .rounded(px(crate::theme::R_CHIP))
                                    .object_fit(gpui::ObjectFit::Contain),
                            ),
                    ),
            );
        let content = crate::motion::dialog_in("attachment-preview-in", sheet);
        // The scrim is the window's, as under Settings and the Project
        // sheet: the nav and the titlebar dim too. The sheet itself stays
        // centred on, and sized by, the Pane that owns it.
        let window_size = window.viewport_size();
        gpui::base::Dialog::new(cx)
            .focus_handle(preview.focus.clone())
            .left(px(0.))
            .top(px(0.))
            .w(window_size.width)
            .h(window_size.height)
            .backdrop(crate::motion::veil_in(
                "attachment-preview-veil",
                div()
                    .debug_selector(|| "attachment-preview-scrim".into())
                    .size_full(),
            ))
            .popup(
                div()
                    .absolute()
                    .left(bounds.origin.x)
                    .top(bounds.origin.y)
                    .w(bounds.size.width)
                    .h(bounds.size.height)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(content),
            )
            .request_close(move |_, window, cx| close_dialog.close(window, cx))
            .into_any_element()
    }
}

/// The sheet's size for an image of `natural` pixels in a Pane of
/// `pane_w` × `pane_h`: within `min(0.9 × pane_w, PREVIEW_MAX_W)` ×
/// `0.85 × pane_h`, the image area (inside the head and `MODAL_PAD`) takes
/// the image's aspect ratio, never scaled up past its own size. Until the
/// image is decoded, the sheet takes the whole box.
fn sheet_size(natural: Option<(f32, f32)>, pane_w: f32, pane_h: f32) -> (f32, f32) {
    use crate::theme::{MODAL_HEAD_H, MODAL_PAD, PREVIEW_MAX_W};
    let max_w = (pane_w * 0.9).clamp(0., PREVIEW_MAX_W);
    let max_h = (pane_h * 0.85).max(0.);
    // The frame around the image: the sheet's 1px edges, the head and its
    // rule, and the inset on every side.
    let frame_w = 2. + 2. * MODAL_PAD;
    let frame_h = 2. + MODAL_HEAD_H + 1. + 2. * MODAL_PAD;
    let Some((image_w, image_h)) = natural.filter(|(w, h)| *w > 0. && *h > 0.) else {
        return (max_w, max_h);
    };
    let room_w = (max_w - frame_w).max(1.);
    let room_h = (max_h - frame_h).max(1.);
    let scale = (room_w / image_w).min(room_h / image_h).min(1.);
    // A tiny image keeps a sheet wide enough for its head.
    let width = (image_w * scale + frame_w).max(PREVIEW_MIN_W.min(max_w));
    (width, image_h * scale + frame_h)
}

/// The narrowest preview sheet: room for a title beside its two controls.
const PREVIEW_MIN_W: f32 = 320.;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_takes_the_image_aspect_inside_the_pane_box() {
        let (w, h) = sheet_size(Some((1600., 900.)), 1000., 800.);
        assert!(w <= 768. && h <= 680.);
        let frame_h = 2. + crate::theme::MODAL_HEAD_H + 1. + 2. * crate::theme::MODAL_PAD;
        let frame_w = 2. + 2. * crate::theme::MODAL_PAD;
        let ratio = (w - frame_w) / (h - frame_h);
        assert!((ratio - 16. / 9.).abs() < 0.01, "{ratio}");
        // A tall image is bounded by the height.
        let (_, tall) = sheet_size(Some((400., 4000.)), 1000., 800.);
        assert!((tall - 680.).abs() < 0.5);
        // Never scaled up; still wide enough for the head.
        assert_eq!(
            sheet_size(Some((100., 50.)), 1000., 800.),
            (320., 50. + frame_h)
        );
        // Undecoded: the whole box.
        assert_eq!(sheet_size(None, 1000., 800.), (768., 680.));
    }
}
