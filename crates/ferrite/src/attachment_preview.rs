//! The reader slot a Pane owns: what opens beside it on the board, in a
//! slot of its own laid out like a Pane (`cockpit/beside.rs` draws it).
//! Its document is a text file (numbered lines with the target line marked,
//! or Markdown rendered), a review of the Thread's changes (`review.rs`) or
//! an image (`image_viewer.rs`), each under the same head. This module holds
//! the open document and draws the slot's shell; the cockpit lays it out.
//! The Pane's Composer and transcript share it, so a file or an image opened
//! from either replaces whatever the slot holds.

use crate::pointer::Pointer as _;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use gpui::{
    div, prelude::*, px, rgb, AnyElement, App, Div, Entity, FocusHandle, Focusable as _, Window,
};

use crate::image_viewer::ImageViewer;

#[derive(Clone)]
pub struct Document {
    pub path: PathBuf,
    pub title: String,
    pub body: Body,
}

/// What a document shows.
#[derive(Clone)]
pub enum Body {
    /// A UTF-8 file's text (or text no file holds, a command's output),
    /// opened at `line` (1-based): marked and scrolled into view (F-14).
    /// `None` opens at the top.
    Text { source: String, line: Option<u32> },
    /// A review (ADR 0021): the Thread's changes as a diff that takes
    /// comments. The document's `path` is the workspace then.
    Review(Entity<crate::review::ReviewView>),
    /// An image, `gallery[at]`: ← → step among `gallery`, the images it was
    /// opened among (a prompt's), itself alone when opened on its own.
    Image {
        viewer: Entity<ImageViewer>,
        gallery: Arc<[PathBuf]>,
        at: usize,
    },
}

#[derive(Default)]
struct State {
    document: Option<Document>,
}

#[derive(Clone)]
// Native Markdown callbacks require Send + Sync captures; the Pane still owns this slot.
pub struct Preview {
    state: Arc<Mutex<State>>,
    /// Tracked by the reader slot, so the cockpit can tell a selection in
    /// the reader from a click that should hand focus back to the Pane.
    reader_focus: FocusHandle,
}

impl Preview {
    pub fn new(cx: &mut App) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            reader_focus: cx.focus_handle(),
        }
    }

    /// Whether text inside the open reader holds focus — Markdown, code or
    /// plain text being selected in. The slot itself taking focus on a click
    /// is not text, and does not count.
    pub fn reader_text_focused(&self, window: &Window, cx: &App) -> bool {
        self.reader_focus.contains_focused(window, cx) && !self.reader_focus.is_focused(window)
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
        self.open_source(path, title, source);
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
        self.set(Document {
            path,
            title,
            body: Body::Text { source, line },
        });
        true
    }

    /// Show text no file holds (a command's output) in the reader, as if
    /// read from `path`: the path picks the reader's chip and highlighting.
    pub fn open_source(&self, path: PathBuf, title: String, source: String) {
        self.set(Document {
            path,
            title,
            body: Body::Text { source, line: None },
        });
    }

    /// Show a review in the reader's slot, its head the workspace's name.
    pub fn open_review(
        &self,
        workspace: PathBuf,
        title: String,
        review: Entity<crate::review::ReviewView>,
    ) {
        self.set(Document {
            path: workspace,
            title,
            body: Body::Review(review),
        });
    }

    /// Open `path` as an image in the reader slot. Every way an image opens
    /// comes here — a sent image or a prompt's chip, a chip on the
    /// Composer's shelf, a file link in an answer, a ⌘-clicked path — so
    /// they all behave alike. ← → step among `gallery` when it holds `path`
    /// (a prompt's images). An image already open swaps in place, in the
    /// same viewer; any other document gives way, as opening another file
    /// does. The viewer takes the keyboard, so esc closes it again.
    pub fn open_image(
        &self,
        path: PathBuf,
        gallery: &[PathBuf],
        window: &mut Window,
        cx: &mut App,
    ) {
        let gallery: Arc<[PathBuf]> = if gallery.contains(&path) {
            gallery.into()
        } else {
            Arc::from([path.clone()])
        };
        let at = gallery.iter().position(|image| *image == path).unwrap_or(0);
        let viewer = match self.image() {
            Some(viewer) => viewer,
            None => cx.new(|cx| ImageViewer::new(path, cx)),
        };
        window.focus(&viewer.focus_handle(cx), cx);
        self.show_image(viewer, gallery, at, cx);
        window.refresh();
    }

    /// ← → in an image: the image `by` steps along its gallery, if there is
    /// one there.
    pub fn step_image(&self, by: isize, cx: &mut App) -> bool {
        let Some(Body::Image {
            viewer,
            gallery,
            at,
        }) = self.document().map(|document| document.body)
        else {
            return false;
        };
        let to = at.saturating_add_signed(by).min(gallery.len() - 1);
        if to == at {
            return false;
        }
        self.show_image(viewer, gallery, to, cx);
        true
    }

    /// `gallery[at]` in the slot, its file's name for the head.
    fn show_image(
        &self,
        viewer: Entity<ImageViewer>,
        gallery: Arc<[PathBuf]>,
        at: usize,
        cx: &mut App,
    ) {
        let path = gallery[at].clone();
        viewer.update(cx, |viewer, cx| viewer.show(path.clone(), cx));
        self.set(Document {
            title: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path,
            body: Body::Image {
                viewer,
                gallery,
                at,
            },
        });
    }

    fn set(&self, document: Document) {
        self.state.lock().unwrap().document = Some(document);
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

    /// The review in the reader slot, when that is what it shows.
    pub fn review(&self) -> Option<Entity<crate::review::ReviewView>> {
        match &self.state.lock().unwrap().document.as_ref()?.body {
            Body::Review(review) => Some(review.clone()),
            _ => None,
        }
    }

    /// The image viewer in the reader slot, when that is what it shows.
    fn image(&self) -> Option<Entity<ImageViewer>> {
        match &self.state.lock().unwrap().document.as_ref()?.body {
            Body::Image { viewer, .. } => Some(viewer.clone()),
            _ => None,
        }
    }

    /// The keyboard focus the open document holds, kept from its first
    /// frame: a review's (or a field in it), or the image viewer's.
    pub fn held_focus(&self, window: &Window, cx: &App) -> Option<FocusHandle> {
        if let Some(review) = self.review() {
            return review.read(cx).held_focus(window, cx);
        }
        self.image()?.read(cx).held_focus(window)
    }

    pub fn close_document(&self, window: &mut Window) {
        self.state.lock().unwrap().document = None;
        window.refresh();
    }

    /// The open document as a board slot (F-14): a Pane-shaped shell on the
    /// plane whose head row is handed to `head` (the cockpit wires the drag
    /// that moves the slot) and whose body is the rendered `body`. The head
    /// is the Group head's twin — `PANE_HEAD_H` over its `paint::HAIRLINE`,
    /// its insets and gap — reading the file's path (its `title`) in
    /// `LINK_INK` and its `:line` in `TEXT_DIM` (an image: its name, and
    /// where it stands among its gallery's, `· 2/3`), or `review` and the
    /// workspace, with a dim × door at the right — an image's original (the
    /// OS opens it) the door just before it. None while no document is open.
    pub fn reader(&self, body: AnyElement, head: impl FnOnce(Div) -> AnyElement) -> Option<Div> {
        use crate::theme::*;
        let document = self.document()?;
        let label = match &document.body {
            Body::Review(_) => div()
                .min_w_0()
                .truncate()
                .text_color(rgb(TEXT_DIM))
                .child(format!("review \u{b7} {}", document.title)),
            Body::Text { line, .. } => {
                titled(&document.title).children(line.map(|line| suffix(format!(":{line}"))))
            }
            Body::Image { gallery, at, .. } => {
                titled(&document.title).children((gallery.len() > 1).then(|| {
                    suffix(format!(" \u{b7} {}/{}", at + 1, gallery.len()))
                        .id("reader-image-count")
                        .tooltip(|window, cx| {
                            gpui::component::tooltip::Tooltip::new(
                                "Step through them with \u{2190} \u{2192}",
                            )
                            .build(window, cx)
                        })
                }))
            }
        };
        let original = match &document.body {
            Body::Image { .. } => Some(document.path.clone()),
            _ => None,
        };
        let close = self.clone();
        // Hover blends per reader: two readers on a Group never share one.
        let key = Arc::as_ptr(&self.state) as usize;
        let head_band = div()
            .debug_selector(|| "reader-head".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(PANE_HEAD_GAP))
            .h(px(PANE_HEAD_H + 1.))
            .pl(px(PANE_HEAD_PAD_L))
            .pr(px(PANE_HEAD_PAD_R))
            .border_b_1()
            .border_color(paint::HAIRLINE)
            .whitespace_nowrap()
            .child(label)
            .child(div().flex_1())
            .children(original.map(|original| {
                head_control("open-original-image", format!("reader-original-{key}"))
                    .tooltip(|window, cx| {
                        gpui::component::tooltip::Tooltip::new(
                            "Open the original in the default app",
                        )
                        .build(window, cx)
                    })
                    .child(crate::icons::icon(
                        crate::icons::EXTERNAL,
                        ICON_BUTTON_GLYPH,
                        TEXT_DIM,
                    ))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        open_original(&original, window, cx);
                    })
            }))
            .child(
                head_control("close-markdown-reader", format!("reader-close-{key}"))
                    .tooltip(|window, cx| {
                        gpui::component::tooltip::Tooltip::new("Close the reader \u{b7} esc")
                            .build(window, cx)
                    })
                    .child(crate::icons::icon(
                        crate::icons::CLOSE,
                        ICON_BUTTON_GLYPH,
                        TEXT_DIM,
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
                .bg(paint::CARD)
                .font_family(FONT_UI)
                .text_size(px(FS_UI))
                .line_height(px(LH_UI))
                .child(head(head_band))
                .child(body),
        )
    }
}

/// The head's title: a path in `LINK_INK`, cut before its suffix is and
/// from its start, so a narrow reader keeps the file's name.
fn titled(title: &str) -> Div {
    use crate::theme::LINK_INK;
    div().flex().min_w_0().overflow_hidden().child(
        div()
            .min_w_0()
            .overflow_hidden()
            .text_ellipsis_start()
            .text_color(rgb(LINK_INK))
            .child(title.to_string()),
    )
}

/// What follows the title, dim and never cut: a `:line`, an image's place.
fn suffix(text: String) -> Div {
    div()
        .flex_shrink_0()
        .text_color(rgb(crate::theme::TEXT_DIM))
        .child(text)
}

/// One of the head's doors at the right, the Group head's fill button's
/// twin: an `ICON_BUTTON` square at `R_TAB`, quiet until the pointer is on
/// it. A press here acts; it must not also pick the slot up as a drag.
fn head_control(id: &'static str, hover: String) -> gpui::Stateful<Div> {
    use crate::theme::{ICON_BUTTON, R_TAB};
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(ICON_BUTTON))
        .rounded(px(R_TAB))
        .hover_control(hover)
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// The OS opens the original file in its own viewer, where it can be
/// inspected at full resolution. Canonicalize before URL encoding so relative
/// paths, symlinks and Windows drive paths all use the same file-link route.
pub(crate) fn open_original(path: &Path, window: &mut Window, cx: &mut App) {
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
        match &self.body {
            Body::Review(_) => return "DIFF".into(),
            Body::Text { .. } if self.is_markdown() => return "MARKDOWN".into(),
            Body::Text { .. } => {
                if let Some(language) = ferrite_core::transcript::language_for_path(&self.path) {
                    return match language {
                        "cpp" => "C++".into(),
                        language => language.to_ascii_uppercase(),
                    };
                }
            }
            Body::Image { .. } => {}
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

    /// A text document's source; nothing for a review or an image.
    pub fn source(&self) -> &str {
        match &self.body {
            Body::Text { source, .. } => source,
            _ => "",
        }
    }

    /// The line a text document opened at.
    pub fn line(&self) -> Option<u32> {
        match self.body {
            Body::Text { line, .. } => line,
            _ => None,
        }
    }
}
