//! Pane-owned file previews: images use a focused overlay while text files
//! open in a reader that takes its own slot on the board, beside its Pane
//! (`cockpit/beside.rs` draws it: numbered lines, the target line marked).
//! A compare of the branch against its base rides the same slot. The kit
//! owns dialog focus and dismissal; this module supplies the owning Pane's
//! bounds, which place and size the image sheet, while the scrim covers the
//! window like every modal's.

use crate::components::Tip as _;
use crate::pointer::Pointer as _;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use gpui::{
    canvas, div, prelude::*, px, rgb, AnyElement, App, Bounds, Div, FocusHandle, IntoElement,
    Pixels, Window,
};

#[derive(Clone)]
pub struct Document {
    pub path: PathBuf,
    pub title: String,
    pub source: String,
    /// The line the reader opened at (1-based): marked and scrolled into
    /// view (F-14). `None` opens at the top.
    pub line: Option<u32>,
    /// A compare (F-15): the branch's diff against its base, drawn in the
    /// reader's slot in place of a file. `source` is empty then.
    pub compare: Option<Compare>,
}

/// A branch against its base, parsed from `git diff <base>...HEAD`.
#[derive(Clone, Debug, PartialEq)]
pub struct Compare {
    /// `dev ↔ main · 3 files +40 −12`: the slot's head.
    pub head: String,
    pub edits: Vec<ferrite_core::FileEdit>,
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
            line: None,
            compare: None,
        });
        window.refresh();
        true
    }

    /// Open a UTF-8 text file in the reader at `line` (F-14): the line is
    /// marked and the cockpit scrolls it into view. `false` for a binary
    /// file or one that cannot be read.
    pub fn open_text_at(&self, path: PathBuf, title: String, line: Option<u32>) -> bool {
        let Ok(bytes) = std::fs::read(&path) else {
            return false;
        };
        let Ok(source) = String::from_utf8(bytes) else {
            return false;
        };
        self.state.lock().unwrap().document = Some(Document {
            path,
            title,
            source,
            line,
            compare: None,
        });
        true
    }

    /// Show a compare in the reader's slot (F-15).
    pub fn open_compare(&self, workspace: PathBuf, compare: Compare) {
        self.state.lock().unwrap().document = Some(Document {
            path: workspace,
            title: compare.head.clone(),
            source: String::new(),
            line: None,
            compare: Some(compare),
        });
    }

    /// Close the reader with no window in hand (a test, the cockpit's own
    /// key handler).
    pub fn close_reader(&self) {
        self.state.lock().unwrap().document = None;
    }

    /// Whether the reader slot (or anything in it) holds focus — what lets
    /// ⌘W close the reader rather than its Pane.
    pub fn reader_focused(&self, window: &Window, cx: &App) -> bool {
        self.reader_focus.contains_focused(window, cx)
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
    /// its own on the board (`cockpit/beside.rs`), laid out like a Pane.
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

    /// The open document as a board slot (F-14): a Pane-shaped shell on the
    /// plane whose head row is handed to `head` (the cockpit wires the drag
    /// that moves the slot) and whose body is the rendered `body`. The head
    /// is the Group head's twin — `PANE_HEAD_H`, its `paint::LINE` rule, a
    /// cell in from each side — reading the file's path (its `title`) in
    /// `PATH_INK` and its `:line` in `TEXT_MUTED`, or a compare's own head
    /// line, with a dim × at the right. None while no document is open.
    pub fn reader(&self, body: AnyElement, head: impl FnOnce(Div) -> AnyElement) -> Option<Div> {
        use crate::theme::*;
        let document = self.document()?;
        let label = match &document.compare {
            Some(compare) => div()
                .min_w_0()
                .truncate()
                .text_color(rgb(TEXT_MUTED))
                .child(compare.head.clone()),
            None => div()
                .flex()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(PATH_INK))
                        .child(document.title.clone()),
                )
                .children(document.line.map(|line| {
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(TEXT_MUTED))
                        .child(format!(":{line}"))
                })),
        };
        let close = self.clone();
        let head_band = div()
            .debug_selector(|| "reader-head".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(PANE_HEAD_H))
            .px(px(HEAD_PAD_X))
            .border_b_1()
            .border_color(paint::LINE)
            .whitespace_nowrap()
            .child(label)
            .child(div().flex_1())
            .child(
                div()
                    .id("close-markdown-reader")
                    .debug_selector(|| "close-markdown-reader".into())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .w(px(ICON_BUTTON))
                    .h(px(PANE_HEAD_H))
                    .hover_control("reader-close")
                    // A press here closes; it must not also pick the slot
                    // up as a drag.
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .tooltip(|window, cx| {
                        gpui::component::tooltip::Tooltip::new("Close the reader \u{b7} esc")
                            .build(window, cx)
                    })
                    .child(crate::icons::icon(
                        crate::icons::CLOSE,
                        GLYPH_BOX,
                        TEXT_MUTED,
                    ))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        close.close_document(window);
                    }),
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
                .bg(paint::PLANE)
                .font_family(FONT_UI)
                .text_size(px(FS_UI))
                .line_height(px(LH_UI))
                .child(head(head_band))
                .child(body),
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
        // The sheet recipe (`prefs::sheet`): `paint::BAND`, a `paint::LINE2`
        // edge, square, the float's one shadow; the 48px head with its title and the
        // one close control over a hairline; then the image itself on the
        // sheet, `MODAL_PAD` in from every edge. No well: nothing in a
        // sheet is darker than the sheet.
        // A word on the head, `paint::HOVER` under the pointer.
        let open = crate::components::form_button("open-original-attachment", cx)
            .flex_shrink_0()
            .h(px(crate::theme::ROW))
            .px(px(crate::theme::CH))
            .child(crate::components::form_label(
                "open original",
                crate::theme::TEXT_MUTED,
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
