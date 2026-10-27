//! A Pane's preview: the file, image or command output opened from
//! anywhere in the Pane, shown in the Panel's Preview tab
//! (`cockpit/panel/preview.rs` draws it; SPEC §5.6.3). Its document is a
//! text file (numbered lines with the target line marked, or Markdown
//! rendered), an image (`image_viewer.rs`), or a file it cannot show (a
//! binary file, one that could not be read), the original a click away.
//!
//! The Pane's transcript, Composer and rendered documents share it, so a
//! file or an image opened from any of them replaces whatever it holds:
//! [`Preview::open`] is the one way a file opens, and the cockpit routes
//! its own entry points (a file link, a ⌘-clicked path, a changed file, a
//! review's line) there too. The cockpit sees what opened on its next
//! frame and shows the Panel on it.
//!
//! The Review is no preview: it reads beside its Pane on the board, in the
//! reader slot (`cockpit/beside.rs`, ADR 0021). This module also holds the
//! head both reading surfaces share ([`reading_head`]).

use ferrite_ui::pointer::Pointer as _;
use std::{
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use gpui::{
    div, prelude::*, px, rgb, App, Div, Entity, FocusHandle, Focusable as _, SharedString, Window,
};

use crate::image_viewer::ImageViewer;

#[derive(Clone)]
pub struct Document {
    pub path: PathBuf,
    /// What the head reads for text no file holds (a command's name); a
    /// file's head reads its path, from the Thread's workspace.
    pub name: Option<String>,
    pub body: Body,
}

/// What a document shows.
#[derive(Clone)]
pub enum Body {
    /// A UTF-8 file's text (or text no file holds, a command's output),
    /// opened at `line` (1-based): marked and scrolled into view (F-14).
    /// `None` opens at the top.
    Text { text: Arc<Text>, line: Option<u32> },
    /// An image, `gallery[at]`: ← → step among `gallery`, the images it was
    /// opened among (a prompt's), itself alone when opened on its own.
    Image {
        viewer: Entity<ImageViewer>,
        gallery: Arc<[PathBuf]>,
        at: usize,
    },
    /// A file the preview cannot show, and why.
    Unshown(Unshown),
}

/// Why a file is not shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unshown {
    /// Its bytes are not UTF-8 text.
    Binary,
    /// It could not be read: the error.
    Unreadable(SharedString),
}

/// A text document's source, read once and shared: drawing it, finding in
/// it or rendering it never copies it, and its lines are found once.
pub struct Text {
    pub source: SharedString,
    /// Each line's bytes in `source`, as `str::lines` splits it.
    lines: Vec<Range<usize>>,
}

impl Text {
    pub fn new(source: String) -> Self {
        let mut lines = Vec::new();
        let mut start = 0;
        for line in source.split_inclusive('\n') {
            let text = line.strip_suffix('\n').unwrap_or(line);
            let text = text.strip_suffix('\r').unwrap_or(text);
            lines.push(start..start + text.len());
            start += line.len();
        }
        Self {
            source: source.into(),
            lines,
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Line `at` (0-based).
    pub fn line(&self, at: usize) -> &str {
        &self.source[self.lines[at].clone()]
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(|range| &self.source[range.clone()])
    }
}

#[derive(Default)]
struct State {
    document: Option<Document>,
    /// The document just closed, still drawn while the Panel rides out
    /// with it; the cockpit lets it go once nothing draws it (`let_go`).
    closing: Option<Document>,
    /// Counts every open, so the cockpit tells a new one from the last.
    opened: u64,
}

#[derive(Clone)]
// Native Markdown callbacks require Send + Sync captures; the Pane still owns this preview.
pub struct Preview {
    state: Arc<Mutex<State>>,
    /// Tracked by the Preview tab's card, so the cockpit can tell a
    /// selection in it from a click that hands focus back to the Pane.
    focus: FocusHandle,
}

impl Preview {
    pub fn new(cx: &mut App) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            focus: cx.focus_handle(),
        }
    }

    /// Open `path` (F-14): the one way a file opens in the preview. An
    /// image opens in the image viewer, ← → stepping among `gallery` when
    /// it holds `path` (a prompt's images); any other file as text, at
    /// `line` when there is one — marked, and scrolled into view — or, when
    /// it is no text or cannot be read, as a note saying so with its
    /// original a click away. Whatever was open gives way.
    pub fn open(
        &self,
        path: PathBuf,
        line: Option<u32>,
        gallery: &[PathBuf],
        window: &mut Window,
        cx: &mut App,
    ) {
        if crate::image_viewer::is_image(&path) {
            self.open_image(path, gallery, window, cx);
        } else {
            let body = match std::fs::read(&path) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(source) => Body::Text {
                        text: Arc::new(Text::new(source)),
                        line,
                    },
                    Err(_) => Body::Unshown(Unshown::Binary),
                },
                Err(error) => Body::Unshown(Unshown::Unreadable(error.to_string().into())),
            };
            self.set(Document {
                path,
                name: None,
                body,
            });
        }
        window.refresh();
    }

    /// Show text no file holds (a command's output) as if read from
    /// `path`, under `name`: the path picks its highlighting.
    pub fn open_source(&self, path: PathBuf, name: String, source: String) {
        self.set(Document {
            path,
            name: Some(name),
            body: Body::Text {
                text: Arc::new(Text::new(source)),
                line: None,
            },
        });
    }

    /// An image: one already open swaps in place, in the same viewer, which
    /// takes the keyboard, so esc closes it again.
    fn open_image(&self, path: PathBuf, gallery: &[PathBuf], window: &mut Window, cx: &mut App) {
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
            path,
            name: None,
            body: Body::Image {
                viewer,
                gallery,
                at,
            },
        });
    }

    fn set(&self, document: Document) {
        let mut state = self.state.lock().unwrap();
        state.document = Some(document);
        state.closing = None;
        state.opened += 1;
    }

    /// Close the preview (esc in it, its ×, ⌘W while it holds focus). It
    /// is closed at once — `document` is `None` — while the document it
    /// showed stays drawable (`shown`) until the cockpit lets it go.
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(document) = state.document.take() {
            state.closing = Some(document);
        }
    }

    /// Nothing draws the closed document any more: stop holding it.
    pub fn let_go(&self) {
        self.state.lock().unwrap().closing = None;
    }

    pub fn document(&self) -> Option<Document> {
        self.state.lock().unwrap().document.clone()
    }

    /// Whether a document is open.
    pub fn is_open(&self) -> bool {
        self.state.lock().unwrap().document.is_some()
    }

    /// The open document, else the one just closed.
    pub fn shown(&self) -> Option<Document> {
        let state = self.state.lock().unwrap();
        state.document.clone().or_else(|| state.closing.clone())
    }

    /// Which open this is, and which document for telling one opening over
    /// another (`swap_key`), read without copying it; `None` while closed.
    pub fn opened(&self) -> Option<(u64, u64)> {
        let state = self.state.lock().unwrap();
        let document = state.document.as_ref()?;
        Some((state.opened, swap_key(document)))
    }

    /// The image viewer, when that is what is open.
    fn image(&self) -> Option<Entity<ImageViewer>> {
        match &self.state.lock().unwrap().document.as_ref()?.body {
            Body::Image { viewer, .. } => Some(viewer.clone()),
            _ => None,
        }
    }

    /// The focus the Preview tab's card tracks.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Whether the preview (or anything in it) holds the keyboard — what
    /// lets ⌘W close the preview rather than its Pane.
    pub fn focused(&self, window: &Window, cx: &App) -> bool {
        self.is_open() && self.focus.contains_focused(window, cx)
    }

    /// Whether text inside the open preview holds focus — Markdown, code or
    /// plain text being selected in. The card itself taking focus on a
    /// click is not text, and nor is the image viewer, which holds the
    /// keyboard of its own (`held_focus`).
    pub fn text_focused(&self, window: &Window, cx: &App) -> bool {
        self.focused(window, cx)
            && !self.focus.is_focused(window)
            && self
                .image()
                .is_none_or(|viewer| !viewer.focus_handle(cx).contains_focused(window, cx))
    }

    /// The keyboard focus the open document holds, kept from its first
    /// frame: the image viewer's.
    pub fn held_focus(&self, window: &Window, cx: &App) -> Option<FocusHandle> {
        self.image()?.read(cx).held_focus(window)
    }
}

/// Which document a preview shows, for telling one opening over another:
/// its path and line. Every image is one document here: the viewer
/// crossfades one image into the next itself, so its body does not fade
/// again.
fn swap_key(document: &Document) -> u64 {
    use std::hash::{Hash, Hasher};
    if matches!(document.body, Body::Image { .. }) {
        return 0;
    }
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    document.path.hash(&mut hash);
    document.line().hash(&mut hash);
    hash.finish()
}

impl Document {
    pub fn is_markdown(&self) -> bool {
        self.path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
            })
    }

    /// Whether it renders as Markdown: a Markdown file opened at no line (a
    /// file opened at a line reads as numbered lines, to mark it).
    pub fn renders_markdown(&self) -> bool {
        matches!(self.body, Body::Text { line: None, .. }) && self.is_markdown()
    }

    /// A text document's text; nothing for an image or a file not shown.
    pub fn text(&self) -> Option<&Arc<Text>> {
        match &self.body {
            Body::Text { text, .. } => Some(text),
            _ => None,
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

// -------------------------------------------------------------- the head

/// The head both reading surfaces wear — the Panel's preview card and the
/// Review's slot on the board: the Group head's twin, `PANE_HEAD_H` over
/// its `paint::HAIRLINE`, its insets and gap, so it lines up with the head
/// of the Pane beside it. `label` at the left, `doors` (`head_door`s) at
/// the right.
pub fn reading_head(label: Div, doors: impl IntoIterator<Item = gpui::AnyElement>) -> Div {
    use ferrite_ui::theme::*;
    div()
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
        .children(doors)
}

/// A head's title: a path in `LINK_INK`, cut from its start before its
/// suffix (`head_suffix`) is, so a narrow head keeps the file's name.
pub fn head_title(title: impl Into<SharedString>) -> Div {
    use ferrite_ui::theme::LINK_INK;
    div().flex().min_w_0().overflow_hidden().child(
        div()
            .min_w_0()
            .overflow_hidden()
            .text_ellipsis_start()
            .text_color(rgb(LINK_INK))
            .child(title.into()),
    )
}

/// What follows a head's title, dim and never cut: a `:line`, an image's
/// place.
pub fn head_suffix(text: String) -> Div {
    div()
        .flex_shrink_0()
        .text_color(rgb(ferrite_ui::theme::TEXT_DIM))
        .child(text)
}

/// One of a head's doors at the right, the Group head's fill button's
/// twin: an `ICON_BUTTON` square at `R_TAB` holding `glyph`, quiet until
/// the pointer is on it (`hover`, its blend's key). A press here acts; it
/// must not also pick the slot up as a drag.
pub fn head_door(id: &'static str, hover: String, glyph: &'static str) -> gpui::Stateful<Div> {
    use ferrite_ui::theme::{ICON_BUTTON, ICON_BUTTON_GLYPH, R_TAB, TEXT_DIM};
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
        .child(ferrite_ui::icons::icon(glyph, ICON_BUTTON_GLYPH, TEXT_DIM))
}

/// The OS opens the original file in its own viewer, where it can be
/// inspected at full resolution. Canonicalize before URL encoding so relative
/// paths, symlinks and Windows drive paths all use the same file-link route.
pub fn open_original(path: &Path, window: &mut Window, cx: &mut App) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A text's lines split as `str::lines` splits them, `\r\n` and a last
    /// line with no newline included, and read back without copying.
    #[test]
    fn a_text_finds_its_lines_once() {
        let source = "one\r\ntwo\n\nfour";
        let text = Text::new(source.into());
        assert_eq!(
            text.lines().collect::<Vec<_>>(),
            source.lines().collect::<Vec<_>>()
        );
        assert_eq!(text.line_count(), 4);
        assert_eq!(text.line(1), "two");
        assert_eq!(Text::new("a\n".into()).line_count(), 1);
        assert_eq!(Text::new(String::new()).line_count(), 0);
    }
}
