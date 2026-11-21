//! A Pane's preview: the file or command output opened from anywhere in
//! the Pane, shown in the Panel's Preview tab (`cockpit/panel/preview.rs`
//! draws it; SPEC §5.6.3), and the image it opened in the lightbox. Its
//! document is a text file (numbered lines with the target line marked, or
//! Markdown rendered), a page drawn live (`page.rs`: an `.html` file or a
//! local address, opened from its transcript card by
//! [`Preview::open_page`]), or a file it cannot show (a binary file, one
//! that could not be read), the original a click away. An image is no document:
//! it opens over the whole window in the lightbox (`cockpit/lightbox.rs`
//! draws it, `image_viewer.rs` its stage), leaving the document as it was.
//!
//! The Pane's transcript, Composer and rendered documents share it, so a
//! file opened from any of them replaces whatever it holds, and an image
//! the lightbox's: [`Preview::open`] is the one way a file opens, and the
//! cockpit routes its own entry points (a file link, a ⌘-clicked path, a
//! changed file, a review's line) there too. The cockpit sees what opened
//! on its next frame and shows the Panel or the lightbox on it.
//!
//! The Review is no preview: it reads beside its Pane on the board, in the
//! reader slot (`cockpit/beside.rs`, ADR 0021). This module also holds the
//! head both reading surfaces share ([`reading_head`]).

use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use gpui::{
    div, prelude::*, px, rgb, App, Div, Entity, FocusHandle, Focusable as _, SharedString, Window,
};

use crate::image_viewer::ImageViewer;
use crate::page::{PageSource, PageView};

/// The path the live browser's document stands under (`show_browser`).
pub const BROWSER_PATH: &str = "browser://tabs";

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
    /// A page, drawn live where the engine can.
    Page(Arc<PageView>),
    /// The Thread-owned browser; the window adapter supplies its exact live page.
    Browser,
    BrowserAnnotation {
        annotation: Arc<ferrite_core::browser_annotations::Annotation>,
        image: Option<PathBuf>,
    },
    /// A file the preview cannot show, and why.
    Unshown(Unshown),
}

/// An image open in the lightbox, `gallery[at]`: ← → step among `gallery`,
/// the images it was opened among (a prompt's), itself alone when opened
/// on its own. One viewer shows them all, crossfading one into the next.
#[derive(Clone)]
pub struct Lightbox {
    pub viewer: Entity<ImageViewer>,
    pub gallery: Arc<[PathBuf]>,
    pub at: usize,
    /// Where in the window the picture was opened from, as fractions of it:
    /// the lightbox grows from that spot (the pointer's at the open).
    pub from: (f32, f32),
    /// The zoom menu under the bar's zoom door is open.
    pub zoom_menu: bool,
}

impl Lightbox {
    /// The image shown.
    pub fn path(&self) -> &Path {
        &self.gallery[self.at]
    }
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
    /// The line with the most characters: what a reader sizes its
    /// sideways scroll by.
    widest: usize,
    /// Its words as Markdown, when it reads as prose (`prose_markdown`):
    /// made on first ask, then shared.
    prose: std::sync::OnceLock<SharedString>,
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
        let widest = lines
            .iter()
            .enumerate()
            .max_by_key(|(_, range)| source[(*range).clone()].chars().count())
            .map_or(0, |(at, _)| at);
        Self {
            source: source.into(),
            lines,
            widest,
            prose: std::sync::OnceLock::new(),
        }
    }

    /// Its words as Markdown (`prose_markdown`), made once.
    pub fn prose(&self) -> &SharedString {
        self.prose
            .get_or_init(|| prose_markdown(&self.source).into())
    }

    /// The index of the line with the most characters (0 for no lines).
    pub fn widest_line(&self) -> usize {
        self.widest
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileOpen {
    pub path: PathBuf,
    pub line: Option<u32>,
}

#[derive(Default)]
struct State {
    remote_files: bool,
    file_requests: Vec<FileOpen>,
    attachments: HashMap<PathBuf, AttachmentPreview>,
    next_attachment_request: u64,
    attachment_open: Option<(FileOpen, Vec<PathBuf>)>,
    document: Option<Document>,
    /// Counts every open, so the cockpit tells a new one from the last.
    opened: u64,
    lightbox: Option<Lightbox>,
    browser_urls: Vec<String>,
    annotation_requests: Vec<ferrite_core::browser_annotations::Annotation>,
    annotation_edits: Vec<ferrite_core::browser_annotations::Annotation>,
}

enum AttachmentPreview {
    Queued,
    Loading(u64),
    Ready(PathBuf),
    Failed(String),
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

    pub fn set_remote_files(&self, remote: bool) {
        let mut state = self.state.lock().unwrap();
        if state.remote_files != remote {
            state.attachments.clear();
            state.attachment_open = None;
        }
        state.remote_files = remote;
    }

    pub fn remote_files(&self) -> bool {
        self.state.lock().unwrap().remote_files
    }

    /// Queue a remote attachment's preview for the window's pump, and ring
    /// its bell when the preview is new (ADR 0041).
    pub fn request_attachment(&self, path: &Path) {
        let mut state = self.state.lock().unwrap();
        if state.remote_files
            && ferrite_core::attachment_transfer::is_retained_path(path)
            && !state.attachments.contains_key(path)
        {
            state
                .attachments
                .insert(path.to_path_buf(), AttachmentPreview::Queued);
            ferrite_core::doorbell::ring();
        }
    }

    pub fn attachment_local(&self, path: &Path) -> Option<PathBuf> {
        if !self.remote_files() {
            return Some(path.to_path_buf());
        }
        self.request_attachment(path);
        match self.state.lock().unwrap().attachments.get(path) {
            Some(AttachmentPreview::Ready(local)) => Some(local.clone()),
            _ => None,
        }
    }

    pub fn attachment_failed(&self, path: &Path) -> bool {
        matches!(
            self.state.lock().unwrap().attachments.get(path),
            Some(AttachmentPreview::Failed(_))
        )
    }

    pub fn take_attachment_requests(&self) -> Vec<(PathBuf, u64)> {
        let mut state = self.state.lock().unwrap();
        let active = state
            .attachments
            .values()
            .filter(|entry| matches!(entry, AttachmentPreview::Loading(_)))
            .count();
        let queued: Vec<_> = state
            .attachments
            .iter()
            .filter_map(|(path, entry)| {
                matches!(entry, AttachmentPreview::Queued).then_some(path.clone())
            })
            .take(2usize.saturating_sub(active))
            .collect();
        queued
            .into_iter()
            .map(|path| {
                state.next_attachment_request += 1;
                let request = state.next_attachment_request;
                state
                    .attachments
                    .insert(path.clone(), AttachmentPreview::Loading(request));
                (path, request)
            })
            .collect()
    }

    pub fn finish_attachment(&self, path: &Path, request: u64, result: Result<PathBuf, String>) {
        let mut state = self.state.lock().unwrap();
        if matches!(state.attachments.get(path), Some(AttachmentPreview::Loading(current)) if *current == request)
        {
            state.attachments.insert(
                path.to_path_buf(),
                match result {
                    Ok(local) => AttachmentPreview::Ready(local),
                    Err(reason) => AttachmentPreview::Failed(reason),
                },
            );
        }
    }

    pub fn settle_attachment_open(&self, window: &mut Window, cx: &mut App) {
        let ready = {
            let mut state = self.state.lock().unwrap();
            let Some((request, gallery)) = state.attachment_open.as_ref() else {
                return;
            };
            let result = match state.attachments.get(&request.path) {
                Some(AttachmentPreview::Ready(local)) => Ok(local.clone()),
                Some(AttachmentPreview::Failed(reason)) => Err(reason.clone()),
                _ => return,
            };
            let local_gallery = gallery
                .iter()
                .filter_map(|path| match state.attachments.get(path) {
                    Some(AttachmentPreview::Ready(local)) => Some(local.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            (
                state.attachment_open.take().unwrap().0,
                local_gallery,
                result,
            )
        };
        match ready {
            (request, gallery, Ok(local)) => {
                self.open_local(local, request.line, &gallery, window, cx)
            }
            (request, _, Err(reason)) => {
                self.set(Document {
                    path: request.path,
                    name: None,
                    body: Body::Unshown(Unshown::Unreadable(reason.into())),
                });
                window.refresh();
            }
        }
    }

    pub fn take_file_requests(&self) -> Vec<FileOpen> {
        std::mem::take(&mut self.state.lock().unwrap().file_requests)
    }

    pub fn review_annotation(
        &self,
        annotation: ferrite_core::browser_annotations::Annotation,
        window: &mut Window,
    ) {
        self.state
            .lock()
            .unwrap()
            .annotation_requests
            .push(annotation);
        window.refresh();
    }

    /// Edit an already retained capture independently of its original live
    /// page. The owning window resolves its immutable image asynchronously.
    pub fn edit_annotation(&self, annotation: ferrite_core::browser_annotations::Annotation) {
        self.state.lock().unwrap().annotation_edits.push(annotation);
        ferrite_core::doorbell::ring();
        self.show_browser();
    }

    pub fn take_annotation_edits(&self) -> Vec<ferrite_core::browser_annotations::Annotation> {
        std::mem::take(&mut self.state.lock().unwrap().annotation_edits)
    }

    pub fn take_annotation_requests(&self) -> Vec<ferrite_core::browser_annotations::Annotation> {
        std::mem::take(&mut self.state.lock().unwrap().annotation_requests)
    }

    pub fn loading_annotation(
        &self,
        annotation: ferrite_core::browser_annotations::Annotation,
        window: &mut Window,
    ) {
        self.set(Document {
            path: annotation.original().map(Path::to_path_buf).unwrap_or_default(),
            name: Some("Browser annotation".into()),
            body: Body::BrowserAnnotation {
                annotation: Arc::new(annotation),
                image: None,
            },
        });
        window.refresh();
    }

    pub fn open_annotation(
        &self,
        annotation: ferrite_core::browser_annotations::Annotation,
        image: PathBuf,
        window: &mut Window,
    ) {
        self.set(Document {
            path: annotation.original().map(Path::to_path_buf).unwrap_or_default(),
            name: Some("Browser annotation".into()),
            body: Body::BrowserAnnotation {
                annotation: Arc::new(annotation),
                image: Some(image),
            },
        });
        window.refresh();
    }

    pub fn annotation_error(
        &self,
        annotation: &ferrite_core::browser_annotations::Annotation,
        error: String,
        window: &mut Window,
    ) {
        self.set(Document {
            path: annotation.original().map(Path::to_path_buf).unwrap_or_default(),
            name: Some("Browser annotation".into()),
            body: Body::Unshown(Unshown::Unreadable(error.into())),
        });
        window.refresh();
    }

    /// Open `path` (F-14): the one way a file opens in the preview. An
    /// image opens in the lightbox, ← → stepping among `gallery` when it
    /// holds `path` (a prompt's images), and the document stays as it was;
    /// any other file as text, at `line` when there is one — marked, and
    /// scrolled into view — or, when it is no text or cannot be read, as a
    /// note saying so with its original a click away, the document open
    /// before giving way.
    pub fn open(
        &self,
        path: PathBuf,
        line: Option<u32>,
        gallery: &[PathBuf],
        window: &mut Window,
        cx: &mut App,
    ) {
        {
            let mut state = self.state.lock().unwrap();
            if state.remote_files {
                if ferrite_core::attachment_transfer::is_retained_path(&path) {
                    if matches!(
                        state.attachments.get(&path),
                        None | Some(AttachmentPreview::Failed(_))
                    ) {
                        state
                            .attachments
                            .insert(path.clone(), AttachmentPreview::Queued);
                    }
                    state.attachment_open = Some((FileOpen { path, line }, gallery.to_vec()));
                    window.refresh();
                    return;
                }
                state.attachment_open = None;
                state.file_requests.push(FileOpen { path, line });
                window.refresh();
                return;
            }
        }
        self.open_local(path, line, gallery, window, cx);
    }

    pub fn open_local(
        &self,
        path: PathBuf,
        line: Option<u32>,
        gallery: &[PathBuf],
        window: &mut Window,
        cx: &mut App,
    ) {
        self.state.lock().unwrap().attachment_open = None;
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

    /// Open a page beside the transcript (its card's `Open`): a file under
    /// its path, an address under its own name. Whatever was open gives
    /// way.
    pub fn open_page(&self, source: PageSource, window: &mut Window) {
        if let PageSource::Address(url) = &source {
            if self.open_url(url, window) {
                return;
            }
        }
        let (path, name) = match &source {
            PageSource::File(path) => (path.clone(), None),
            PageSource::Address(address) => (PathBuf::from(address), Some(address.clone())),
        };
        self.set(Document {
            path,
            name,
            body: Body::Page(PageView::new(source)),
        });
        window.refresh();
    }

    /// Route ordinary website links through the owning Thread's browser.
    /// Explicit external-browser commands continue to use the OS URL opener.
    pub fn open_url(&self, url: &str, window: &mut Window) -> bool {
        if !url::Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https")) {
            return false;
        }
        // The window's pump opens it: ring its bell (ADR 0041).
        self.state.lock().unwrap().browser_urls.push(url.to_owned());
        ferrite_core::doorbell::ring();
        self.show_browser();
        window.refresh();
        true
    }

    pub fn show_browser(&self) {
        self.set(Document {
            path: PathBuf::from(BROWSER_PATH),
            name: Some("Browser".into()),
            body: Body::Browser,
        });
    }

    /// Requests retain their Pane's Thread identity when the window drains them.
    pub fn take_browser_urls(&self) -> Vec<String> {
        std::mem::take(&mut self.state.lock().unwrap().browser_urls)
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

    /// An image, in the lightbox: one already open there swaps in place, in
    /// the same viewer, which takes the keyboard, so esc closes it again.
    fn open_image(&self, path: PathBuf, gallery: &[PathBuf], window: &mut Window, cx: &mut App) {
        let gallery: Arc<[PathBuf]> = if gallery.contains(&path) {
            gallery.into()
        } else {
            Arc::from([path.clone()])
        };
        let at = gallery.iter().position(|image| *image == path).unwrap_or(0);
        let open = self.lightbox();
        let from = open.as_ref().map_or_else(
            || {
                let (at, size) = (window.mouse_position(), window.viewport_size());
                (
                    (f32::from(at.x) / f32::from(size.width)).clamp(0., 1.),
                    (f32::from(at.y) / f32::from(size.height)).clamp(0., 1.),
                )
            },
            |lightbox| lightbox.from,
        );
        let viewer = match open.map(|lightbox| lightbox.viewer) {
            Some(viewer) => viewer,
            None => cx.new(|cx| ImageViewer::new(path, cx)),
        };
        window.focus(&viewer.focus_handle(cx), cx);
        self.show_image(viewer, gallery, at, from, cx);
    }

    /// ← → in the lightbox: the image `by` steps along its gallery, if
    /// there is one there.
    pub fn step_image(&self, by: isize, cx: &mut App) -> bool {
        let Some(Lightbox {
            viewer,
            gallery,
            at,
            from,
            ..
        }) = self.lightbox()
        else {
            return false;
        };
        let to = at.saturating_add_signed(by).min(gallery.len() - 1);
        if to == at {
            return false;
        }
        self.show_image(viewer, gallery, to, from, cx);
        true
    }

    fn show_image(
        &self,
        viewer: Entity<ImageViewer>,
        gallery: Arc<[PathBuf]>,
        at: usize,
        from: (f32, f32),
        cx: &mut App,
    ) {
        let path = gallery[at].clone();
        viewer.update(cx, |viewer, cx| viewer.show(path, cx));
        self.state.lock().unwrap().lightbox = Some(Lightbox {
            viewer,
            gallery,
            at,
            from,
            zoom_menu: false,
        });
    }

    /// Open or shut the lightbox's zoom menu.
    pub fn set_zoom_menu(&self, open: bool) {
        if let Some(lightbox) = self.state.lock().unwrap().lightbox.as_mut() {
            lightbox.zoom_menu = open;
        }
    }

    /// The image open in the lightbox, if one is.
    pub fn lightbox(&self) -> Option<Lightbox> {
        self.state.lock().unwrap().lightbox.clone()
    }

    /// Close the lightbox (esc, its ×, a press on the dim around it).
    pub fn close_lightbox(&self) {
        self.state.lock().unwrap().lightbox = None;
    }

    /// Show `document` again as it was last read (its Panel tab brought
    /// back), without reading its file afresh.
    pub fn restore(&self, document: Document) {
        self.set(document);
    }

    fn set(&self, document: Document) {
        let mut state = self.state.lock().unwrap();
        state.attachment_open = None;
        state.document = Some(document);
        state.opened += 1;
    }

    /// Close the preview (esc in it, its ×, ⌘W while it holds focus).
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.attachment_open = None;
        state.document = None;
    }

    pub fn document(&self) -> Option<Document> {
        self.state.lock().unwrap().document.clone()
    }

    /// Whether a document is open.
    pub fn is_open(&self) -> bool {
        self.state.lock().unwrap().document.is_some()
    }

    /// Which open this is, and which document for telling one opening over
    /// another (`swap_key`), read without copying it; `None` while closed.
    pub fn opened(&self) -> Option<(u64, u64)> {
        let state = self.state.lock().unwrap();
        let document = state.document.as_ref()?;
        Some((state.opened, swap_key(document)))
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
    /// click is not text.
    pub fn text_focused(&self, window: &Window, cx: &App) -> bool {
        self.focused(window, cx) && !self.focus.is_focused(window)
    }

    /// The keyboard focus the lightbox holds, kept from its first frame:
    /// its viewer's.
    pub fn held_focus(&self, window: &Window, cx: &App) -> Option<FocusHandle> {
        self.lightbox()?.viewer.read(cx).held_focus(window)
    }
}

/// Files with no extension that are code, not prose.
const BUILD_FILES: &[&str] = &[
    "Makefile",
    "makefile",
    "GNUmakefile",
    "Dockerfile",
    "Containerfile",
    "Justfile",
    "justfile",
    "Rakefile",
    "Gemfile",
    "Procfile",
    "Vagrantfile",
    "Brewfile",
    "BUILD",
    "WORKSPACE",
];

/// Plain text as Markdown that renders exactly its words: every ASCII
/// punctuation mark escaped (they are text, never markup), each line
/// break a hard break, a blank line a paragraph's end, and a line's
/// leading spaces kept as no-break spaces (Markdown would drop them, or
/// read four as code).
pub fn prose_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let words = line.trim_start_matches(' ');
        out.extend(std::iter::repeat_n('\u{a0}', line.len() - words.len()));
        for ch in words.trim_end().chars() {
            if ch.is_ascii_punctuation() {
                out.push('\\');
            }
            out.push(ch);
        }
        let Some(next) = lines.peek() else {
            break;
        };
        let ends_paragraph = words.trim().is_empty() || next.trim().is_empty();
        out.push_str(if ends_paragraph { "\n" } else { "\\\n" });
    }
    out
}

/// Which document a preview shows, for telling one opening over another:
/// its path and line.
fn swap_key(document: &Document) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    document.path.hash(&mut hash);
    document.line().hash(&mut hash);
    hash.finish()
}

fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
}

/// Plain text a reader reads as prose: a `.txt`, or a file with no
/// extension and no build file's name.
fn is_prose_path(path: &Path) -> bool {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => {
            extension.eq_ignore_ascii_case("txt") || extension.eq_ignore_ascii_case("text")
        }
        None => path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| !name.starts_with('.') && !BUILD_FILES.contains(&name)),
    }
}

/// What a Markdown or prose file's `source` renders as, as Markdown; nothing
/// for a file that reads as code.
pub fn reading_markup(path: &Path, source: &str) -> Option<String> {
    if is_markdown_path(path) {
        Some(source.to_string())
    } else if is_prose_path(path) {
        Some(prose_markdown(source))
    } else {
        None
    }
}

impl Document {
    pub fn is_markdown(&self) -> bool {
        is_markdown_path(&self.path)
    }

    /// Whether it is prose, not code: a file of plain text (`.txt`, or no
    /// extension and no build file's name), never a command's output. It
    /// reads as Claude's file viewer reads prose: the reading face at the
    /// reading size, wrapped, no line numbers.
    pub fn is_prose(&self) -> bool {
        self.name.is_none() && is_prose_path(&self.path)
    }

    /// The Markdown it renders, when it renders as a reading document
    /// rather than numbered lines: a Markdown file's own source, or prose's
    /// words escaped into Markdown — opened at no line (a file opened at a
    /// line reads as numbered lines, to mark it).
    pub fn rich_source(&self) -> Option<SharedString> {
        let Body::Text { text, line: None } = &self.body else {
            return None;
        };
        if self.is_markdown() {
            Some(text.source.clone())
        } else if self.is_prose() {
            Some(text.prose().clone())
        } else {
            None
        }
    }

    /// The page it shows, if it is one.
    pub fn page(&self) -> Option<&Arc<PageView>> {
        match &self.body {
            Body::Page(page) => Some(page),
            _ => None,
        }
    }

    /// A text document's text; nothing for a page or a file not shown.
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
/// Review's slot on the board: `PANE_HEAD_H` over its `paint::LINE`, at
/// its insets and gap. `label` at the left, `doors` (icon buttons) at the
/// right.
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
        .border_color(paint::LINE)
        .whitespace_nowrap()
        .text_size(px(FS_UI))
        .line_height(px(LH_UI))
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
        .text_color(rgb(ferrite_ui::theme::INK_SUBTLE))
        .child(text)
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

    fn receipt(name: &str) -> PathBuf {
        PathBuf::from(format!("/server/attachments/{}/{name}", "a".repeat(64)))
    }

    #[gpui::test]
    fn remote_attachment_cache_is_separate_from_local_paths_and_other_previews(
        cx: &mut gpui::TestAppContext,
    ) {
        let first = cx.update(Preview::new);
        let other = cx.update(Preview::new);
        first.set_remote_files(true);
        other.set_remote_files(true);
        let path = receipt("screenshot.png");
        let local = PathBuf::from("/local-cache/screenshot.png");
        assert_eq!(first.attachment_local(&path), None);
        let requests = first.take_attachment_requests();
        assert_eq!(requests[0].0, path);
        first.finish_attachment(&path, requests[0].1, Ok(local.clone()));
        assert_eq!(first.attachment_local(&path), Some(local));
        assert_eq!(other.attachment_local(&path), None);
        assert_eq!(other.take_attachment_requests()[0].0, path);
        let workspace = PathBuf::from("/server/workspace/image.png");
        assert_eq!(first.attachment_local(&workspace), None);
        assert!(first.take_attachment_requests().is_empty());
    }

    #[gpui::test]
    fn remote_attachment_preview_fetches_are_bounded_and_stale_answers_are_discarded(
        cx: &mut gpui::TestAppContext,
    ) {
        let preview = cx.update(Preview::new);
        preview.set_remote_files(true);
        for name in ["a.png", "b.png", "c.png"] {
            preview.request_attachment(&receipt(name));
        }
        let pending = preview.take_attachment_requests();
        assert_eq!(pending.len(), 2);
        assert!(preview.take_attachment_requests().is_empty());
        preview.finish_attachment(&pending[0].0, pending[0].1, Err("Disconnected".into()));
        assert!(preview.attachment_failed(&pending[0].0));
        assert_eq!(preview.take_attachment_requests().len(), 1);
        preview.set_remote_files(false);
        preview.set_remote_files(true);
        assert_eq!(preview.attachment_local(&pending[1].0), None);
        let next = preview.take_attachment_requests();
        preview.finish_attachment(
            &pending[1].0,
            pending[1].1,
            Ok("/old-machine/cache.png".into()),
        );
        assert_eq!(preview.attachment_local(&pending[1].0), None);
        preview.finish_attachment(
            &next[0].0,
            next[0].1,
            Ok("/current-machine/cache.png".into()),
        );
        assert_eq!(
            preview.attachment_local(&pending[1].0),
            Some("/current-machine/cache.png".into())
        );
    }

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
