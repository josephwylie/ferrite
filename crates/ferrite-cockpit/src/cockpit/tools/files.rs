//! Files (Claude's explorer, `files-empty.png`, `files-code.png`,
//! `files-more-menu.png`): the Panel's Files Surface (ADR 0039), two
//! columns — the open files' editor at the left, the checkout's tree at the
//! right.
//!
//! - **Its row** (under the Panel's head): the editor's own file tabs (a
//!   preview tab, one a press opened and the next press replaces, reads in
//!   italic; typing or a second press keeps it), then `Go to file` (⌘P,
//!   the tree's filter), `Collapse all folders`, `Hide file tree` (⇧⌘Y)
//!   and `⋯`.
//! - **Tree** (`tree`): a filter field, folders that open on a press, git's
//!   colour on each name; a right-click offers `Attach as context`, `Copy ›`
//!   and `Show in Finder`.
//! - **Editor** (`editor`): a text file edits, with Auto save. Prose (a
//!   `.md` or `.txt`) opens as the reader reads it — the reading face,
//!   Markdown rendered — and `Show source` swaps in the editor; a line to
//!   go to opens the source. An image opens in the Pane's lightbox, as one
//!   in the conversation does; any other file that is no text names its
//!   kind and offers its own app.
//! - **Tabs:** they scroll sideways when they overflow, a chevron at each
//!   end with more, and the shown one is always brought into view. Each is
//!   a stop in the Panel's keyboard row (`panel/controls.rs`): ←→ step, ⏎
//!   shows, ⌫ and ⌘W close.
//! - **Empty:** `Open files appear here`.
//! - **`⋯`:** Show file tree, Preview tabs, Auto save, Word wrap, Tab size.
//!
//! Every way a file opens in the right pane — a path in the conversation,
//! `Open in Files`, the tree — arrives at `open_in_files`.
//!
//! Every Thread's Files live in one view of their own (ADR 0041), which
//! the Panel draws as the Files Surface's body. What it draws of the
//! window it reads through `files_inputs`; what it asks of the window (a
//! file opened from the tree, Show changes, Attach, a menu, ⌘P) it says
//! as a `FilesEvent`. Making a Thread's Files needs its checkout, the
//! machine and the tabs it was left with, so the window makes them and
//! keeps the saved tabs (`WorkingSets`, files.json).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::WeakEntity;

#[path = "remote_files.rs"]
pub(in crate::cockpit::tools) mod remote_files;
use ferrite_core::file_operations::Reply as FileReply;
use remote_files::RemoteFiles;

use ferrite_pane::composer::{Composer, Role};
use ferrite_pane::preview::{open_original, reading_markup, Body as DocBody};
use crate::cockpit::menu_button::{Door, Doors, Edge, MenuHost};
use ferrite_ui::components::{icon_button, IconSize, Tip as _};
use ferrite_ui::icons::{self, icon};
use ferrite_ui::theme::{self};

use super::changes::ChangesView;
use super::editor::{self, Editor};
use super::save::SaveState;
use super::tree::{FileTree, Row, Status};
use super::working_set::{OpenFile, WorkingSet};
use ferrite_core::SavedThread;
use super::*;
use crate::cockpit::context_menu::Submenu;
use crate::cockpit::panel::controls::focusable;
use crate::cockpit::panel::preview::{settle_markdown_found, PreviewFind};
use crate::cockpit::panel::{PanelControl, PanelFocus, Semantics};
use crate::cockpit::surfaces::SurfaceId;
use crate::cockpit::tab_scroll::{self, Side, TabScroll};

/// The filter field's key context: ⏎ opens the first match.
pub(crate) const FILTER_CONTEXT: &str = "FilesFilter";

gpui::actions!(files, [OpenFirst]);

/// What the Files ⋯ menu and the head's doors do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verb {
    ToggleTree,
    CollapseAll,
    /// Read the tree and git's word again.
    Refresh,
    TogglePreviewTabs,
    ToggleAutoSave,
    ToggleWrap,
    TabSize(usize),
    /// A tree row's: attach the file, copy its path, show it in Finder.
    Attach,
    CopyAbsolute,
    CopyRelative,
    CopyName,
}

/// A parent row's submenu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sub {
    Copy,
    TabSize,
}

/// The tab sizes the menu offers.
pub(crate) const TAB_SIZES: [usize; 3] = [2, 4, 8];

enum TabBody {
    Code(Entity<Editor>),
    Unshown(Unshown),
}

#[derive(Clone, PartialEq, Eq)]
enum Unshown {
    Loading,
    Remote(String),
    Unreadable,
    /// Not text: what kind of file it looks like, by its extension.
    Binary(&'static str),
}

impl Unshown {
    fn note(&self) -> &str {
        match self {
            Self::Loading => "Loading remote file…",
            Self::Remote(error) => error,
            Self::Unreadable => "Could not read this file",
            Self::Binary(kind) => kind,
        }
    }
}

/// What a file of known extension is, for the note that stands where no
/// text can.
fn file_kind(path: &Path) -> &'static str {
    if ferrite_pane::image_viewer::is_image(path) {
        return "Image";
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("pdf") => "PDF document",
        Some("mp3" | "wav" | "m4a" | "flac" | "ogg" | "aac") => "Audio file",
        Some("mp4" | "mov" | "webm" | "mkv" | "avi") => "Video file",
        Some("zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar") => "Archive",
        Some("doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "pages" | "numbers" | "key") => {
            "Office document"
        }
        _ => "Binary file",
    }
}

/// How a text file is shown. Markdown and prose read rendered until the
/// reader asks for the source, which is the one that edits.
enum Reading {
    /// Code: the editor, with no rendered form.
    Plain,
    Source,
    Rendered(Rendered),
}

struct Rendered {
    markup: SharedString,
    cache: ferrite_pane::rich::TextCache,
    scroll: ScrollHandle,
    /// ⌘F's bar over the document, while it is open.
    find: Option<PreviewFind>,
}

struct Tab {
    path: PathBuf,
    /// A preview tab: the next open replaces it, until it is edited or
    /// opened twice.
    preview: bool,
    body: TabBody,
    reading: Reading,
    load: u64,
    pending_line: Option<u32>,
}

impl Tab {
    /// The editor, while the source is what shows.
    fn shown_editor(&self) -> Option<&Entity<Editor>> {
        match (&self.body, &self.reading) {
            (TabBody::Code(editor), Reading::Plain | Reading::Source) => Some(editor),
            _ => None,
        }
    }

    /// The source shows, for a line to stand out in.
    fn show_source(&mut self) {
        if matches!(self.reading, Reading::Rendered(_)) {
            self.reading = Reading::Source;
        }
    }
}

struct Files {
    tree: FileTree,
    remote: Option<RemoteFiles>,
    filter: Entity<Composer>,
    tabs: Vec<Tab>,
    active: Option<usize>,
    settings: editor::Settings,
    preview_tabs: bool,
    show_tree: bool,
    tree_scroll: ScrollHandle,
    tab_scroll: TabScroll,
    /// The tree row a right-click is on, for the menu's verbs.
    pressed: Option<PathBuf>,
    /// Counts the git status reads started; only the latest one lands.
    status_reads: u64,
}

impl Files {
    fn active_path(&self) -> Option<&Path> {
        self.active.map(|at| self.tabs[at].path.as_path())
    }

    /// Whether a buffer holds text the disk does not have yet.
    fn holds_edits(&self, cx: &gpui::App) -> bool {
        self.tabs.iter().any(|tab| match &tab.body {
            TabBody::Code(editor) => editor.read(cx).is_dirty(),
            TabBody::Unshown(_) => false,
        })
    }

    fn working_set(&self) -> WorkingSet {
        WorkingSet {
            files: self
                .tabs
                .iter()
                .map(|tab| OpenFile {
                    path: tab.path.clone(),
                    preview: tab.preview,
                })
                .collect(),
            active: self.active,
            settings: self.settings,
            preview_tabs: self.preview_tabs,
            show_tree: self.show_tree,
        }
    }
}


/// Every Thread's Files, and the ones a gone Thread left with an edit the
/// disk does not have. One view, not one per Thread: a gone Thread's
/// buffers stay with it until they are written.
pub(crate) struct FilesView {
    cockpit: WeakEntity<CockpitView>,
    threads: HashMap<ThreadKey, Files>,
    /// Files whose Thread is gone, kept while a buffer holds an edit the
    /// disk does not have: auto save still writes it, a quit still flushes
    /// it, and leaving still names it.
    orphaned: Vec<Files>,
}

/// What Files draws from, as the Cockpit has it this frame.
pub(crate) struct FilesInputs {
    /// The Thread whose Panel shows Files.
    thread: ThreadKey,
    /// Whether the Panel is wide enough for the tree beside the editor.
    beside: bool,
    doors: Doors,
    /// The Panel's row, which the file tabs are stops of.
    focus: Rc<PanelFocus>,
    /// The diffs read, for `Show changes` on a file the last read lists.
    changes: Entity<ChangesView>,
}

/// What Files asks of the window.
pub(crate) enum FilesEvent {
    /// A press in the tree, the filter's ⏎: an image opens in the Pane's
    /// lightbox, any other file in a tab.
    Open { thread: ThreadKey, path: PathBuf, keep: bool },
    /// `Show changes`: the file's diff, in Changes.
    ShowChanges { thread: ThreadKey, path: String },
    /// `Attach as context`: the file in the Thread's Composer, or on a
    /// machine a line naming it.
    Attach { thread: ThreadKey, path: PathBuf, remote: bool },
    /// A right-click on a tree row: its menu, at the press.
    Menu { thread: ThreadKey, at: gpui::Point<Pixels> },
    /// `Go to file or search`: as ⌘P.
    GoToFile { thread: ThreadKey },
    /// A press on the `⋯` button.
    Door(Door),
}

impl EventEmitter<FilesEvent> for FilesView {}

impl MenuHost for FilesView {
    fn press_door(&mut self, door: Door, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(FilesEvent::Door(door));
    }

    fn tip<E: InteractiveElement>(element: E, text: SharedString) -> E {
        <CockpitView as MenuHost>::tip(element, text)
    }
}

impl CockpitView {
    /// Open `path` in the Files editor for `thread` (at `line`, 1-based),
    /// the Files pane shown. `keep` opens it as a tab of its own; else it
    /// is a preview tab that the next open replaces.
    pub(crate) fn open_in_files(
        &mut self,
        thread: ThreadId,
        path: PathBuf,
        line: Option<u32>,
        keep: bool,
        cx: &mut Context<Self>,
    ) {
        let thread = self.cockpit.read(cx).thread_key(thread);
        self.upsert_surface(thread, SurfaceId::Files, cx);
        let beside = self.files_tree_beside(cx);
        self.files
            .update(cx, |files, cx| files.open(thread, path, line, keep, beside, cx));
    }

    /// The Files state for `thread`, made the first time it is asked for:
    /// in the Thread's checkout, on the window's machine, with the tabs and
    /// settings it was left with.
    pub(in crate::cockpit) fn ensure_files(&self, thread: ThreadId, cx: &mut Context<Self>) {
        let key = self.cockpit.read(cx).thread_key(thread);
        if self.files.read(cx).holds(key) {
            return;
        }
        let Some(root) = self.thread_path(thread, cx) else {
            return;
        };
        let machine = self.cockpit.read(cx).machine().cloned();
        let stored = self
            .machine_key
            .clone()
            .and_then(|machine| self.panel.working_sets.get(&SavedThread { machine, thread }).cloned());
        self.files
            .update(cx, |files, cx| files.make(key, root, machine, stored, cx));
    }

    /// ⌘P: the Files Surface shows, and the tree's filter takes the
    /// keyboard.
    pub(in crate::cockpit) fn go_to_file(&mut self, thread: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        let thread = self.cockpit.read(cx).thread_key(thread);
        self.upsert_surface(thread, SurfaceId::Files, cx);
        self.files.update(cx, |files, cx| files.go_to_file(thread, window, cx));
        cx.notify();
    }

    /// ⌘F with the keyboard in the shown Files surface (its tree, tab strip
    /// or file) or on the Panel's controls over it: the find bar opens over
    /// its file, source or rendered. Whether it did. A Panel showing another
    /// surface, or the keyboard in the conversation, is not Files' to answer.
    pub(in crate::cockpit) fn open_files_find(
        &mut self,
        thread: ThreadId,
        on_controls: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.active_surface(self.cockpit.read(cx).thread_key(thread)) != Some(SurfaceId::Files) {
            return false;
        }
        let held = on_controls || self.panel_keys_held(window, cx);
        let thread = self.cockpit.read(cx).thread_key(thread);
        self.files
            .update(cx, |files, cx| files.open_find(thread, held, window, cx))
    }

    pub(in crate::cockpit) fn uses_remote_files(&self, thread: ThreadId, cx: &gpui::App) -> bool {
        self.on_machine(cx) || self.files.read(cx).is_remote(self.cockpit.read(cx).thread_key(thread))
    }

    pub(crate) fn files_leave_error(&self, cx: &gpui::App) -> Option<&'static str> {
        self.files
            .read(cx)
            .unsaved_at_leaving(cx)
            .map(|_| "Save or close edited files before switching environments.")
    }

    /// The Panel's stored working sets take the live Files' tabs and
    /// settings, for files.json, as this view's machine's: its core's
    /// Thread list prunes only that machine's sets.
    pub(in crate::cockpit) fn sync_working_sets(&mut self, cx: &gpui::App) {
        let Some(machine) = self.machine_key.clone() else {
            return;
        };
        let live = self.files.read(cx).working_sets().map(|(thread, set)| {
            let thread = SavedThread {
                machine: machine.clone(),
                thread: thread.thread(),
            };
            (thread, set)
        });
        if !self.panel.working_sets.sync(live) {
            return;
        }
        let known = self.cockpit.read(cx).known_threads();
        self.panel.working_sets.retain(&machine, &known);
    }

    /// Every way a file reaches the right pane: a file opened in a Pane's
    /// preview that is plain text opens here instead (a path in the
    /// conversation, `Open in Files`, a review's line). Pages, a command's
    /// output and what is no text stay with the preview.
    pub(in crate::cockpit) fn redirect_previews_to_files(&mut self, cx: &mut Context<Self>) {
        let opened: Vec<(ThreadId, PathBuf, Option<u32>)> = self
            .panes
            .iter()
            .filter_map(|pane| {
                let thread = pane.thread()?;
                let document = pane.preview.document()?;
                let plain = document.name.is_none()
                    && matches!(document.body, DocBody::Text { .. })
                    && self.thread_path(thread, cx).is_some_and(|root| document.path.starts_with(root));
                plain.then(|| (thread, document.path.clone(), document.line()))
            })
            .collect();
        for (thread, path, line) in opened {
            if let Some(index) = self.pane_for(thread) {
                self.panes[index].preview.close();
            }
            self.open_in_files(thread, path, line, false, cx);
        }
    }

    pub(in crate::cockpit) fn on_files_event(
        &mut self,
        _: &Entity<FilesView>,
        event: &FilesEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FilesEvent::Open { thread, path, keep } => match self.pane_for(thread.thread()) {
                Some(index)
                    if !self.uses_remote_files(thread.thread(), cx)
                        && ferrite_pane::image_viewer::is_image(path) =>
                {
                    self.panes[index].preview.open(path.clone(), None, &[], window, cx);
                    cx.notify();
                }
                _ => self.open_in_files(thread.thread(), path.clone(), None, *keep, cx),
            },
            FilesEvent::ShowChanges { thread, path } => self.show_changes_of(thread.thread(), path, cx),
            FilesEvent::Attach { thread, path, remote } => {
                if let Some(index) = self.pane_for(thread.thread()) {
                    self.panes[index].composer.update(cx, |composer, cx| {
                        if *remote {
                            composer.insert(
                                &format!("\nPlease use the file at {} on this machine.\n", path.display()),
                                cx,
                            );
                        } else {
                            composer.add_files(&[path.clone()], cx);
                        }
                    });
                }
                cx.notify();
            }
            FilesEvent::Menu { thread, at } => {
                self.open_context_menu(MenuTarget::File(thread.thread()), *at, cx)
            }
            FilesEvent::GoToFile { thread } => self.go_to_file(thread.thread(), window, cx),
            FilesEvent::Door(door) => self.toggle_door(*door, cx),
        }
    }

    /// Whether the Panel is wide enough for the tree beside a readable
    /// editor. Narrower, the tree is a page over the editor, and a file
    /// picked from it shows the editor again.
    fn files_tree_beside(&self, cx: &gpui::App) -> bool {
        self.panel_pane(cx)
            .is_some_and(|index| self.panel_rest_w(index, cx) >= theme::FILES_TREE_W + theme::FILES_EDITOR_MIN_W)
    }

    /// Everything Files draws from: the Thread whose Panel shows it.
    fn files_inputs(&self, cx: &gpui::App) -> Option<FilesInputs> {
        let thread = self.cockpit.read(cx).thread_key(self.panes[self.panel_drawn_pane(cx)?].thread()?);
        (self.active_surface(thread) == Some(SurfaceId::Files)).then(|| FilesInputs {
            thread,
            beside: self.files_tree_beside(cx),
            doors: self.doors(),
            focus: self.panel.focus.clone(),
            changes: self.changes.clone(),
        })
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn remote_files_for_test(
        &mut self,
        thread: ThreadId,
        answer: impl Fn(ferrite_core::file_operations::Request) -> std::io::Result<FileReply> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.ensure_files(thread, cx);
        let key = self.cockpit.read(cx).thread_key(thread);
        self.files
            .update(cx, |files, _| files.remote_for_test(key, answer));
        if let Some(index) = self.pane_for(thread) {
            self.panes[index].preview.set_remote_files(true);
        }
    }
}

impl FilesView {
    pub(in crate::cockpit) fn new(cockpit: WeakEntity<CockpitView>) -> Self {
        Self {
            cockpit,
            threads: HashMap::new(),
            orphaned: Vec::new(),
        }
    }

    fn files_of(&mut self, thread: ThreadKey) -> Option<&mut Files> {
        self.threads.get_mut(&thread)
    }

    fn files_ref(&self, thread: ThreadKey) -> Option<&Files> {
        self.threads.get(&thread)
    }

    /// Whether `thread`'s Files are made.
    pub(in crate::cockpit) fn holds(&self, thread: ThreadKey) -> bool {
        self.threads.contains_key(&thread)
    }

    /// Whether `thread`'s Files read and write on a machine.
    fn is_remote(&self, thread: ThreadKey) -> bool {
        self.files_ref(thread).is_some_and(|files| files.remote.is_some())
    }

    /// `thread`'s Files, rooted at its checkout `root`, on `machine` when
    /// the window is on one, with the tabs and settings `stored` kept.
    fn make(
        &mut self,
        thread: ThreadKey,
        root: PathBuf,
        machine: Option<std::sync::Arc<ferrite_core::machine::Bridge>>,
        stored: Option<WorkingSet>,
        cx: &mut Context<Self>,
    ) {
        let remote = machine.map(|machine| RemoteFiles::new(root.clone(), self.cockpit.clone(), machine));
        let filter = cx.new(|cx| {
            let mut field = Composer::new(cx);
            field.set_role(Role::Field(Some(FILTER_CONTEXT)), cx);
            field
        });
        cx.subscribe(
            &filter,
            move |view, field, _: &ferrite_pane::composer::Edited, cx| {
                let text = field.read(cx).text().to_string();
                if let Some(files) = view.files_of(thread) {
                    files.tree.set_filter(text);
                    if files.remote.is_some() {
                        view.refresh(thread, cx);
                    }
                    cx.notify();
                }
            },
        )
        .detach();
        let mut files = Files {
            tree: if remote.is_some() {
                FileTree::remote(root)
            } else {
                FileTree::new(root)
            },
            remote,
            filter,
            tabs: Vec::new(),
            active: None,
            settings: editor::Settings::default(),
            preview_tabs: true,
            show_tree: true,
            tree_scroll: ScrollHandle::new(),
            tab_scroll: TabScroll::default(),
            pressed: None,
            status_reads: 0,
        };
        if let Some(stored) = stored {
            files.settings = stored.settings;
            files.preview_tabs = stored.preview_tabs;
            files.show_tree = stored.show_tree;
            for open in stored.files {
                let tab = Self::read_tab(
                    open.path,
                    open.preview,
                    files.settings,
                    files.remote.clone(),
                    thread,
                    cx,
                );
                files.tabs.push(tab);
            }
            files.active = stored.active.filter(|at| *at < files.tabs.len());
            if let Some(active) = files.active {
                files.tab_scroll.reveal(active);
                let path = files.tabs[active].path.clone();
                files.tree.reveal(&path);
            }
        }
        self.threads.insert(thread, files);
        self.refresh(thread, cx);
    }

    /// Open `path` in `thread`'s editor (see `CockpitView::open_in_files`).
    /// `beside`: the tree stands beside the editor, else it gives way to it.
    pub(in crate::cockpit) fn open(
        &mut self,
        thread: ThreadKey,
        path: PathBuf,
        line: Option<u32>,
        keep: bool,
        beside: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(files) = self.files_of(thread) else {
            return;
        };
        files.tree.reveal(&path);
        if let Some(at) = files.tabs.iter().position(|tab| tab.path == path) {
            files.active = Some(at);
            files.tabs[at].preview &= !keep;
        } else {
            let preview = files.preview_tabs && !keep;
            let settings = files.settings;
            let remote = files.remote.clone();
            let tab = Self::read_tab(path, preview, settings, remote, thread, cx);
            let Some(files) = self.files_of(thread) else {
                return;
            };
            if preview {
                if let Some(at) = files.tabs.iter().position(|tab| tab.preview) {
                    files.tabs.remove(at);
                }
            }
            files.tabs.push(tab);
            files.active = Some(files.tabs.len() - 1);
        }
        let Some(files) = self.files_of(thread) else {
            return;
        };
        if !beside {
            files.show_tree = false;
        }
        let at = files.active.unwrap_or_default();
        files.tab_scroll.reveal(at);
        if let Some(line) = line {
            let tab = &mut files.tabs[at];
            tab.pending_line = Some(line);
            tab.show_source();
            if let Some(editor) = tab.shown_editor() {
                editor.update(cx, |editor, cx| editor.go_to_line(line as usize, cx));
            }
        }
        if files.remote.is_some() {
            self.refresh(thread, cx);
        }
        cx.notify();
    }

    fn read_tab(
        path: PathBuf,
        preview: bool,
        settings: editor::Settings,
        remote: Option<RemoteFiles>,
        thread: ThreadKey,
        cx: &mut Context<Self>,
    ) -> Tab {
        static LOADS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let load = LOADS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(remote) = remote {
            let answer = remote.read(&path, cx);
            let opened = path.clone();
            cx.spawn(async move |view, cx| {
                let result = cx.background_executor().spawn(async move { remote_files::receive(answer).await }).await;
                let _ = view.update(cx, |view, cx| {
                    let Some(files) = view.files_ref(thread).filter(|files| files.remote.as_ref().is_some_and(|current| current.same_workspace(&remote))) else { return; };
                    let Some(at) = files.tabs.iter().position(|tab| tab.load == load && tab.path == opened) else { return; };
                    let settings = files.settings;
                    let line = files.tabs[at].pending_line;
                    let (body, reading) = match result {
                        Ok(FileReply::Text { text, .. }) => {
                            let editor = Self::editor_for(&opened, text, settings, thread, cx);
                            editor.update(cx, |editor, cx| {
                                editor.set_remote(remote);
                                if let Some(line) = line { editor.go_to_line(line as usize, cx); }
                            });
                            (TabBody::Code(editor), Reading::Plain)
                        }
                        Ok(FileReply::Binary) => (TabBody::Unshown(Unshown::Remote("This remote file is not text. Use the remote terminal to open or copy it.".into())), Reading::Plain),
                        Ok(_) => (TabBody::Unshown(Unshown::Remote("Update Ferrite on this machine to open remote files.".into())), Reading::Plain),
                        Err(error) => (TabBody::Unshown(Unshown::Remote(error.to_string())), Reading::Plain),
                    };
                    if let Some(tab) = view.files_of(thread).and_then(|files| files.tabs.get_mut(at)) {
                        tab.body = body;
                        tab.reading = reading;
                    }
                    cx.notify();
                });
            }).detach();
            return Tab {
                path,
                preview,
                body: TabBody::Unshown(Unshown::Loading),
                reading: Reading::Plain,
                load,
                pending_line: None,
            };
        }
        let (body, reading) = match std::fs::read(&path) {
            Err(_) => (TabBody::Unshown(Unshown::Unreadable), Reading::Plain),
            Ok(bytes) => match String::from_utf8(bytes) {
                Err(_) => (TabBody::Unshown(Unshown::Binary(file_kind(&path))), Reading::Plain),
                Ok(source) => {
                    let reading = reading_markup(&path, &source).map_or(Reading::Plain, |markup| {
                        Reading::Rendered(Rendered {
                            markup: markup.into(),
                            cache: Default::default(),
                            scroll: ScrollHandle::new(),
                            find: None,
                        })
                    });
                    (TabBody::Code(Self::editor_for(&path, source, settings, thread, cx)), reading)
                }
            },
        };
        Tab {
            path,
            preview,
            body,
            reading,
            load,
            pending_line: None,
        }
    }

    fn editor_for(
        path: &Path,
        source: String,
        settings: editor::Settings,
        thread: ThreadKey,
        cx: &mut Context<Self>,
    ) -> Entity<Editor> {
        let editor = cx.new(|cx| Editor::new(path, source, settings, cx));
        cx.subscribe(&editor, move |view, editor, event: &editor::Event, cx| {
            let Some(files) = view.files_of(thread) else {
                return;
            };
            let Some(at) = files
                .tabs
                .iter()
                .position(|tab| matches!(&tab.body, TabBody::Code(own) if *own == editor))
            else {
                return;
            };
            match event {
                // Typing keeps a preview tab.
                editor::Event::Edited => files.tabs[at].preview = false,
                editor::Event::Settled if editor.read(cx).is_closing_clean() => {
                    return view.remove_tab(thread, at, cx);
                }
                editor::Event::Settled => {}
                editor::Event::Discard => return view.remove_tab(thread, at, cx),
            }
            cx.notify();
        })
        .detach();
        editor
    }

    /// Swap a Markdown or prose file between its rendered face and its
    /// source, which edits and takes the keyboard.
    fn toggle_reading(&mut self, thread: ThreadKey, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.files_of(thread).and_then(|files| files.tabs.get_mut(at)) else {
            return;
        };
        let TabBody::Code(editor) = &tab.body else {
            return;
        };
        match &tab.reading {
            Reading::Plain => return,
            Reading::Source => {
                let Some(markup) = reading_markup(&tab.path, editor.read(cx).text()) else {
                    return;
                };
                tab.reading = Reading::Rendered(Rendered {
                    markup: markup.into(),
                    cache: Default::default(),
                    scroll: ScrollHandle::new(),
                    find: None,
                });
            }
            Reading::Rendered(_) => {
                tab.reading = Reading::Source;
                let focus = editor.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
            }
        }
        cx.notify();
    }

    /// Read the tree and git's word again: the Files tab is brought forward
    /// again, and what changed in the checkout while it was away shows.
    pub(in crate::cockpit) fn refresh(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let Some(files) = self.files_of(thread) else {
            return;
        };
        files.tree.refresh();
        files.status_reads += 1;
        let read = files.status_reads;
        let root = files.tree.root().to_path_buf();
        if let Some(remote) = files.remote.clone() {
            let (directories, search) = files.tree.remote_request();
            let answer = remote.snapshot(directories, search, cx);
            cx.spawn(async move |view, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { remote_files::receive(answer).await })
                    .await;
                let result = match result {
                    Ok(FileReply::Snapshot(snapshot)) => Ok(snapshot),
                    Ok(_) => Err("Update Ferrite on this machine to browse remote files.".into()),
                    Err(error) => Err(error.to_string()),
                };
                let _ = view.update(cx, |view, cx| {
                    if let Some(files) = view.files_of(thread).filter(|files| {
                        files.status_reads == read
                            && files
                                .remote
                                .as_ref()
                                .is_some_and(|current| current.same_workspace(&remote))
                    }) {
                        files.tree.set_remote(result);
                        cx.notify();
                    }
                });
            })
            .detach();
            cx.notify();
            return;
        }
        cx.spawn(async move |view, cx| {
            let status = {
                let root = root.clone();
                cx.background_executor().spawn(async move { FileTree::read_status(&root) }).await
            };
            view.update(cx, |view, cx| {
                if let Some(files) = view
                    .files_of(thread)
                    .filter(|files| files.status_reads == read && files.tree.root() == root)
                {
                    files.tree.set_status(status);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn select_tab(&mut self, thread: ThreadKey, at: usize, cx: &mut Context<Self>) {
        if let Some(files) = self.files_of(thread) {
            files.active = Some(at);
            files.tab_scroll.reveal(at);
            cx.notify();
        }
    }

    /// Close a tab, unless its text is not on disk: it is written first, and
    /// stays with its status bar when that fails.
    pub(in crate::cockpit) fn close_tab(&mut self, thread: ThreadKey, at: usize, cx: &mut Context<Self>) {
        let Some(files) = self.files_of(thread) else {
            return;
        };
        if let TabBody::Code(editor) = &files.tabs[at].body {
            if !editor.update(cx, |editor, cx| editor.request_close(cx)) {
                return;
            }
        }
        self.remove_tab(thread, at, cx);
    }

    fn reload_tab(&mut self, thread: ThreadKey, at: usize, cx: &mut Context<Self>) {
        let Some(files) = self.files_of(thread) else {
            return;
        };
        let (path, settings) = (files.tabs[at].path.clone(), files.settings);
        let preview = files.tabs[at].preview;
        let remote = files.remote.clone();
        let tab = Self::read_tab(path, preview, settings, remote, thread, cx);
        if let Some(slot) = self
            .files_of(thread)
            .and_then(|files| files.tabs.get_mut(at))
        {
            *slot = tab;
        }
        cx.notify();
    }

    fn remove_tab(&mut self, thread: ThreadKey, at: usize, cx: &mut Context<Self>) {
        let Some(files) = self.files_of(thread) else {
            return;
        };
        files.tabs.remove(at);
        files.active = match files.tabs.len() {
            0 => None,
            len => Some(files.active.map_or(0, |active| active.min(len - 1))),
        };
        if let Some(active) = files.active {
            files.tab_scroll.reveal(active);
        }
        cx.notify();
    }

    pub(in crate::cockpit) fn tab_count(&self, thread: ThreadKey) -> usize {
        self.files_ref(thread).map_or(0, |files| files.tabs.len())
    }

    /// Where `path`'s tab stands in the tabs row.
    pub(in crate::cockpit) fn tab_index(&self, thread: ThreadKey, path: &Path) -> Option<usize> {
        self.files_ref(thread)?.tabs.iter().position(|tab| tab.path == path)
    }

    /// The file whose tab stands at `at`.
    pub(in crate::cockpit) fn tab_path(&self, thread: ThreadKey, at: usize) -> Option<PathBuf> {
        Some(self.files_ref(thread)?.tabs.get(at)?.path.clone())
    }

    /// Bring file tab `at` into view in the tabs row.
    pub(in crate::cockpit) fn reveal_tab(&mut self, thread: ThreadKey, at: usize) {
        if let Some(files) = self.files_of(thread) {
            files.tab_scroll.reveal(at);
        }
    }

    /// Whether a tabs row is mid-ease: the frame must be drawn again.
    pub(in crate::cockpit) fn tabs_scrolling(&self, cx: &gpui::App) -> bool {
        self.threads.values().any(|files| files.tab_scroll.running(cx))
    }

    /// Every Files, a gone Thread's included.
    fn every_files(&self) -> impl Iterator<Item = &Files> {
        self.threads.values().chain(&self.orphaned)
    }

    /// A quit's last chance for text auto save has not yet written.
    pub(in crate::cockpit) fn flush_for_quit(&mut self, cx: &mut Context<Self>) {
        let editors: Vec<_> = self
            .every_files()
            .flat_map(|files| files.tabs.iter())
            .filter_map(|tab| match &tab.body {
                TabBody::Code(editor) => Some(editor.clone()),
                TabBody::Unshown(_) => None,
            })
            .collect();
        for editor in editors {
            editor.update(cx, |editor, _| editor.flush_for_quit());
        }
    }

    /// How many open buffers leaving would lose, or `None` when none.
    pub(in crate::cockpit) fn unsaved_at_leaving(&self, cx: &gpui::App) -> Option<usize> {
        let lost = self
            .every_files()
            .flat_map(|files| files.tabs.iter())
            .filter(|tab| match &tab.body {
                TabBody::Code(editor) => editor.read(cx).lost_by_leaving(),
                TabBody::Unshown(_) => false,
            })
            .count();
        (lost > 0).then_some(lost)
    }

    /// Each live Thread's tabs and settings.
    fn working_sets(&self) -> impl Iterator<Item = (ThreadKey, WorkingSet)> + '_ {
        self.threads
            .iter()
            .map(|(&thread, files)| (thread, files.working_set()))
    }

    /// The buffers of `thread` that hold no edits of their own take their
    /// file's new text.
    pub(in crate::cockpit) fn follow_disk(&self, thread: ThreadKey, cx: &mut Context<Self>) {
        let editors: Vec<_> = self
            .files_ref(thread)
            .into_iter()
            .flat_map(|files| files.tabs.iter())
            .filter_map(|tab| match &tab.body {
                TabBody::Code(editor) => Some(editor.clone()),
                TabBody::Unshown(_) => None,
            })
            .collect();
        for editor in editors {
            editor.update(cx, |editor, cx| editor.follow_disk(cx));
        }
    }

    /// Every Thread `keep` refuses takes its Files with it, but for a buffer
    /// that holds an edit the disk does not have yet.
    pub(in crate::cockpit) fn forget(&mut self, keep: impl Fn(ThreadKey) -> bool, cx: &gpui::App) {
        let gone: Vec<ThreadKey> = self.threads.keys().copied().filter(|&thread| !keep(thread)).collect();
        for thread in gone {
            self.orphaned.extend(self.threads.remove(&thread));
        }
        self.orphaned.retain(|files| files.holds_edits(cx));
    }

    /// ⌘P: the tree's filter takes the keyboard.
    fn go_to_file(&mut self, thread: ThreadKey, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(files) = self.files_of(thread) {
            files.show_tree = true;
            let focus = files.filter.read(cx).focus_target(window, cx);
            window.focus(&focus, cx);
        }
    }

    fn open_first_match(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let first = self.files_of(thread).and_then(|files| {
            files.tree.rows().rows.into_iter().find(|row| !row.entry.dir).map(|row| row.entry.path)
        });
        if let Some(path) = first {
            cx.emit(FilesEvent::Open { thread, path, keep: false });
        }
    }

    /// A Files menu verb (the ⋯ menu's rows, a tree row's menu).
    pub(in crate::cockpit) fn run_verb(&mut self, thread: ThreadKey, verb: Verb, cx: &mut Context<Self>) {
        let Some(files) = self.files_of(thread) else {
            return;
        };
        let pressed = files.pressed.clone();
        let remote = files.remote.is_some();
        match verb {
            Verb::ToggleTree => files.show_tree = !files.show_tree,
            Verb::CollapseAll => files.tree.collapse_all(),
            Verb::Refresh => self.refresh(thread, cx),
            Verb::TogglePreviewTabs => files.preview_tabs = !files.preview_tabs,
            Verb::ToggleAutoSave => files.settings.auto_save = !files.settings.auto_save,
            Verb::ToggleWrap => files.settings.wrap = !files.settings.wrap,
            Verb::TabSize(size) => files.settings.tab_size = size,
            Verb::Attach => {
                if let Some(path) = pressed {
                    cx.emit(FilesEvent::Attach { thread, path, remote });
                }
            }
            Verb::CopyAbsolute | Verb::CopyRelative | Verb::CopyName => {
                let root = files.tree.root().to_path_buf();
                if let Some(path) = pressed {
                    let text = match verb {
                        Verb::CopyAbsolute => path.to_string_lossy().into_owned(),
                        Verb::CopyRelative => path
                            .strip_prefix(&root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .into_owned(),
                        _ => path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    };
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
        }
        // The editors follow the menu's settings.
        if let Some(files) = self.files_of(thread) {
            let settings = files.settings;
            for tab in &files.tabs {
                if let TabBody::Code(editor) = &tab.body {
                    editor.update(cx, |editor, cx| editor.configure(settings, cx));
                }
            }
        }
        cx.notify();
    }

    /// The path a tree row's right-click is on (`MenuTarget::File`).
    pub(in crate::cockpit) fn pressed(&self, thread: ThreadKey) -> Option<PathBuf> {
        self.files_ref(thread)?.pressed.clone()
    }

    /// The rows of the Files ⋯ menu.
    pub(in crate::cockpit) fn more_rows(&self, thread: ThreadKey) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(files) = self.files_ref(thread) else {
            return Vec::new();
        };
        let check = |label: &'static str, on: bool, verb: Verb| {
            let mut item = menu::Item::new(label);
            item.checked = on;
            Some((item, MenuVerb::Files(verb)))
        };
        let tree = {
            let chord = ferrite_ui::components::bound_chord("cockpit::ToggleFileTree").unwrap_or_default();
            let mut item = menu::Item::new("Show file tree").shortcut(chord);
            item.checked = files.show_tree;
            Some((item, MenuVerb::Files(Verb::ToggleTree)))
        };
        vec![
            tree,
            None,
            check("Preview tabs", files.preview_tabs, Verb::TogglePreviewTabs),
            check("Auto save", files.settings.auto_save, Verb::ToggleAutoSave),
            None,
            check("Word wrap", files.settings.wrap, Verb::ToggleWrap),
            None,
            Some((
                menu::Item::new("Tab size").detail(files.settings.tab_size.to_string()),
                MenuVerb::Submenu(Submenu::Files(Sub::TabSize)),
            )),
        ]
    }

    pub(in crate::cockpit) fn sub_rows(&self, thread: ThreadKey, sub: Sub) -> Vec<(menu::Item, MenuVerb)> {
        match sub {
            Sub::Copy => [
                ("Absolute path", Verb::CopyAbsolute),
                ("Relative path", Verb::CopyRelative),
                ("Filename", Verb::CopyName),
            ]
            .into_iter()
            .map(|(label, verb)| (menu::Item::new(label), MenuVerb::Files(verb)))
            .collect(),
            Sub::TabSize => {
                let current = self.files_ref(thread).map(|files| files.settings.tab_size);
                TAB_SIZES
                    .into_iter()
                    .map(|size| {
                        let mut item = menu::Item::new(size.to_string());
                        item.checked = current == Some(size);
                        (item, MenuVerb::Files(Verb::TabSize(size)))
                    })
                    .collect()
            }
        }
    }

    /// The tree row menu's rows. On a machine there is no Finder to show
    /// the file in.
    pub(in crate::cockpit) fn row_rows(on_machine: bool) -> Vec<Option<(menu::Item, MenuVerb)>> {
        vec![
            Some((menu::Item::new("Attach as context"), MenuVerb::Files(Verb::Attach))),
            Some((menu::Item::new("Copy"), MenuVerb::Submenu(Submenu::Files(Sub::Copy)))),
            None,
            (!on_machine).then(|| (menu::Item::new("Show in Finder"), MenuVerb::Reveal)),
        ]
    }

    // ------------------------------------------------------------ keyboard

    /// ⌘F in `thread`'s Files; `held`: the keyboard is in its surface or
    /// on the Panel's controls over it. The shown source's own bar, while
    /// held or while the editor has the keyboard; else, held, the rendered
    /// file's. Whether one opened.
    fn open_find(&mut self, thread: ThreadKey, held: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(editor) = self
            .shown_editor(thread)
            .filter(|editor| held || editor.read(cx).holds_keyboard(window, cx))
        {
            editor.update(cx, |editor, cx| editor.open_find(window, cx));
            return true;
        }
        if !held {
            return false;
        }
        let Some((path, rendered)) = self.shown_rendered(thread) else {
            return false;
        };
        let find = rendered.find.get_or_insert_with(|| {
            PreviewFind::new(
                ferrite_pane::find::Find::new(cx, move |view: &mut Self, cx| {
                    view.refind_rendered(thread, cx)
                }),
                path,
            )
        });
        find.bar.focus(window, cx);
        cx.notify();
        true
    }

    /// The shown tab's path and rendered face, while it reads rendered.
    fn shown_rendered(&mut self, thread: ThreadKey) -> Option<(PathBuf, &mut Rendered)> {
        let files = self.files_of(thread)?;
        let tab = &mut files.tabs[files.active?];
        match &mut tab.reading {
            Reading::Rendered(rendered) => Some((tab.path.clone(), rendered)),
            _ => None,
        }
    }

    /// The rendered bar's line changed: search the document again, a new
    /// query starting on its first occurrence in view.
    fn refind_rendered(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let Some((_, Rendered { markup, find: Some(find), .. })) = self.shown_rendered(thread) else {
            return;
        };
        let source = gpui::base::text::TextView::markdown_find_text(markup);
        if find.bar.search([source.as_str()], 0, cx) && find.bar.active().is_some() {
            find.reveal_when_painted(true);
        }
        cx.notify();
    }

    fn step_rendered_find(&mut self, thread: ThreadKey, forward: bool, cx: &mut Context<Self>) {
        let Some((_, Rendered { find: Some(find), .. })) = self.shown_rendered(thread) else {
            return;
        };
        if find.bar.step(forward).is_some() {
            find.reveal_when_painted(false);
            cx.notify();
        }
    }

    fn close_rendered_find(&mut self, thread: ThreadKey, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((_, rendered)) = self.shown_rendered(thread) {
            rendered.find = None;
            window.blur(cx);
            cx.notify();
        }
    }

    /// Once per frame: settle `thread`'s shown rendered file's active
    /// occurrence into view.
    pub(in crate::cockpit) fn tend_prose_find(&mut self, thread: ThreadKey, window: &mut Window) {
        if let Some((_, Rendered { scroll, find: Some(find), .. })) = self.shown_rendered(thread) {
            settle_markdown_found(find, scroll, window);
        }
    }

    /// The shown tab's editor, while its source is what shows.
    pub(in crate::cockpit) fn shown_editor(&self, thread: ThreadKey) -> Option<Entity<Editor>> {
        let files = self.files_ref(thread)?;
        files.tabs[files.active?].shown_editor().cloned()
    }

    /// The shown tab's rendered face's scroll, while it reads rendered.
    pub(in crate::cockpit) fn prose_scroll(&self, thread: ThreadKey) -> Option<ScrollHandle> {
        let files = self.files_ref(thread)?;
        match &files.tabs[files.active?].reading {
            Reading::Rendered(rendered) => Some(rendered.scroll.clone()),
            _ => None,
        }
    }

    // ------------------------------------------------------------ drawing

    /// The Files pane: its head over the editor and the tree.
    fn pane(&self, inputs: &FilesInputs, cx: &mut Context<Self>) -> Div {
        let thread = inputs.thread;
        let Some(files) = self.files_ref(thread) else {
            return div().flex_1();
        };
        let beside = inputs.beside;
        let tree = files.show_tree.then(|| self.files_tree(thread, files, cx));
        div()
            .debug_selector(|| "files".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.files_head(inputs, files, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.files_editor(inputs, files, cx))
                    .children(tree.map(|tree| {
                        if beside {
                            tree.w(px(theme::FILES_TREE_W))
                        } else {
                            tree.absolute().inset_0().bg(theme::paint::PANEL)
                        }
                    })),
            )
    }

    fn files_head(&self, inputs: &FilesInputs, files: &Files, cx: &mut Context<Self>) -> Div {
        use theme::*;
        let thread = inputs.thread;
        files.tab_scroll.settle(cx);
        let tabs = files.tabs.iter().enumerate().map(|(at, tab)| {
            let active = files.active == Some(at);
            let dirty = matches!(&tab.body, TabBody::Code(editor) if editor.read(cx).is_dirty());
            let name = tab
                .path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let id = SharedString::from(format!("files-tab-{at}"));
            let semantics = Semantics::tab(name.clone(), active);
            let handle = inputs.focus.register(PanelControl::FileTab(tab.path.clone()), semantics.clone(), cx);
            focusable(div().id(id.clone()), &handle, &semantics)
                .debug_selector(move || id.to_string())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_1))
                .h(px(PANEL_TAB_H))
                .pl(px(PANEL_TAB_PAD_X))
                .pr(px(SPACE_1))
                .max_w(px(SURFACE_TAB_MAX_W))
                .rounded(px(R_CONTROL))
                .text_size(px(FS_LABEL))
                .text_color(rgb(if active { INK_STRONG } else { INK_SUBTLE }))
                .when(active, |tab| tab.bg(paint::SELECTED))
                .when(tab.preview, |tab| tab.italic())
                .cursor_pointer()
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.select_tab(thread, at, cx)))
                .child(div().min_w_0().truncate().child(SharedString::from(name)))
                .child(
                    icon_button(
                        format!("files-tab-close-{at}"),
                        if dirty { icons::CIRCLE_DASHED } else { icons::CLOSE },
                        IconSize::Compact,
                        "Close",
                        cx,
                    )
                    .tip("Close")
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.close_tab(thread, at, cx);
                    })),
                )
        }).collect::<Vec<_>>();
        let door = |key: &'static str, glyph: &'static str, tip: &'static str, chord: Option<&'static str>| {
            use ferrite_ui::components::{icon_button, IconSize, Tip as _};
            let door = icon_button(key, glyph, IconSize::Standard, tip, cx);
            match chord {
                Some(action) => door.tip_with_key(tip, ferrite_ui::components::bound_chord(action)),
                None => door.tip(tip),
            }
        };
        let chevron = |side: Side| {
            tab_scroll::chevron(
                "files-tabs",
                side,
                &files.tab_scroll,
                cx.listener(move |view, _: &ClickEvent, _, cx| {
                    if let Some(files) = view.files_of(thread) {
                        files.tab_scroll.ease(side, cx);
                    }
                    cx.notify();
                }),
                cx,
            )
        };
        super::surface_bar()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .children(chevron(Side::Left))
                    .child(
                        div()
                            .id("files-tabs")
                            .debug_selector(|| "files-tabs".into())
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(SPACE_0_5))
                            .overflow_x_scroll()
                            .track_scroll(&*files.tab_scroll)
                            .on_scroll_wheel(cx.listener(
                                move |view, event: &gpui::ScrollWheelEvent, window, cx| {
                                    let wheeled = view
                                        .files_of(thread)
                                        .is_some_and(|files| files.tab_scroll.wheel(event, window));
                                    if wheeled {
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                },
                            ))
                            .when(files.tabs.is_empty(), |strip| {
                                strip.text_size(px(FS_LABEL)).text_color(rgb(INK_SUBTLE)).child("Files")
                            })
                            .children(tabs),
                    )
                    .children(chevron(Side::Right)),
            )
            .child(
                door("files-go", icons::SEARCH, "Go to file or search", Some("cockpit::GoToFile"))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(FilesEvent::GoToFile { thread });
                    })),
            )
            .child(
                door("files-refresh", icons::REFRESH, "Refresh file tree", None)
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.run_verb(thread, Verb::Refresh, cx);
                    })),
            )
            .child(
                door("files-collapse", icons::COLLAPSE_ALL, "Collapse all folders", None)
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.run_verb(thread, Verb::CollapseAll, cx);
                    })),
            )
            .child(
                door("files-tree-toggle", icons::LIST, "Hide file tree", Some("cockpit::ToggleFileTree"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.run_verb(thread, Verb::ToggleTree, cx);
                    })),
            )
            .child(inputs.doors.button(
                ferrite_ui::components::icon_button("files-more", icons::ELLIPSIS, IconSize::Standard, "More", cx),
                Door::new(MenuTarget::FilesMore(thread.thread()), Edge::Right),
                "More",
                cx,
            ))
    }

    fn files_editor(&self, inputs: &FilesInputs, files: &Files, cx: &mut Context<Self>) -> Div {
        use theme::*;
        let thread = inputs.thread;
        let column = div()
            .debug_selector(|| "files-editor".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0();
        let Some(tab) = files.active.map(|at| &files.tabs[at]) else {
            return column.child(
                div()
                    .debug_selector(|| "files-empty".into())
                    .flex()
                    .flex_col()
                    .flex_1()
                    .w_full()
                    .items_center()
                    .justify_center()
                    .gap(px(SPACE_2))
                    .px(px(SPACE_6))
                    .min_w_0()
                    .text_center()
                    .child(icon(icons::FOLDER, ICON_BUTTON_GLYPH + 4., INK_SUBTLE))
                    .child(div().w_full().text_size(px(FS_LABEL)).child("Open files appear here"))
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_SUBTLE))
                            .child("Pick a file in the tree, or click a file path in the conversation."),
                    ),
            );
        };
        let relative = tab.path.strip_prefix(files.tree.root()).unwrap_or(&tab.path);
        let relative_path = relative.to_string_lossy().into_owned();
        let changed = inputs.changes.read(cx).lists(thread.thread(), &relative_path);
        let text_action = |key: &'static str, label: &'static str| {
            div()
                .id(key)
                .debug_selector(move || key.into())
                .flex_shrink_0()
                .px(px(SPACE_1))
                .rounded(px(R_CONTROL))
                .cursor_pointer()
                .hover_control(key)
                .text_color(rgb(INK_SUBTLE))
                .child(label)
        };
        let crumbs = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let breadcrumb = div()
            .debug_selector(|| "files-breadcrumb".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(WIN_CHROME_H - SPACE_2))
            .px(px(SIDE_CHAT_INSET + SPACE_2))
            .text_size(px(FS_LABEL))
            .text_color(rgb(INK_SUBTLE))
            .whitespace_nowrap()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(SPACE_1))
                    .overflow_hidden()
                    .children(crumbs.into_iter().enumerate().flat_map(|(at, part)| {
                        [
                            (at > 0).then(|| div().child("/").into_any_element()),
                            Some(div().child(SharedString::from(part)).into_any_element()),
                        ]
                    }).flatten()),
            )
            .children(match &tab.body {
                TabBody::Code(editor) => match editor.read(cx).save_state() {
                    SaveState::Saving(_) => Some("Saving"),
                    SaveState::Saved => Some("Saved"),
                    _ => None,
                },
                _ => None,
            }
            .map(|word| div().debug_selector(|| "files-save-state".into()).flex_shrink_0().child(word)))
            .children((!matches!(tab.reading, Reading::Plain)).then(|| {
                let (key, label) = match tab.reading {
                    Reading::Rendered(_) => ("files-show-source", "Show source"),
                    _ => ("files-show-rendered", "Show rendered"),
                };
                let at = files.active.unwrap_or_default();
                text_action(key, label).on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    view.toggle_reading(thread, at, window, cx)
                }))
            }))
            .children(changed.then(|| {
                let path = relative_path.clone();
                text_action("files-show-changes", "Show changes").on_click(cx.listener(
                    move |_, _: &ClickEvent, _, cx| {
                        cx.emit(FilesEvent::ShowChanges { thread, path: path.clone() })
                    },
                ))
            }))
            .children((files.remote.is_none()).then(|| {
                let file = tab.path.clone();
                text_action("files-open", "Open").on_click(cx.listener(
                    move |_, _: &ClickEvent, _, cx| {
                        if let Ok(url) = url::Url::from_file_path(&file) {
                            cx.open_url(url.as_str());
                        }
                    },
                ))
            }));
        let retry_path = tab.path.clone();
        let open_path = tab.path.clone();
        let body = match (&tab.body, &tab.reading) {
            (TabBody::Code(_), Reading::Rendered(rendered)) => div()
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .id("files-prose")
                        .debug_selector(|| "files-prose".into())
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&rendered.scroll)
                        .px(px(TX_INSET_SOLO))
                        .pt(px(BODY_PAD_T))
                        .pb(px(HALF_ROW))
                        .text_size(px(FS_PROSE))
                        .line_height(px(LH_PROSE))
                        .child(
                            ferrite_pane::rich::Markdown::new(
                                format!("files-{}", tab.path.display()),
                                rendered.markup.clone(),
                                rendered.cache.clone(),
                            )
                            .find(rendered.find.as_ref().and_then(|find| find.bar.marks(0))),
                        ),
                )
                .children(rendered.find.as_ref().map(|find| find.bar.bar()))
                .on_action(cx.listener(move |view, _: &ferrite_pane::find::Next, _, cx| {
                    view.step_rendered_find(thread, true, cx)
                }))
                .on_action(cx.listener(move |view, _: &ferrite_pane::find::Previous, _, cx| {
                    view.step_rendered_find(thread, false, cx)
                }))
                .on_action(cx.listener(move |view, _: &ferrite_pane::find::Close, window, cx| {
                    view.close_rendered_find(thread, window, cx)
                }))
                .into_any_element(),
            (TabBody::Code(editor), _) => div().flex_1().min_h_0().child(editor.clone()).into_any_element(),
            (TabBody::Unshown(why), _) => {
                let why = why.clone();
                div()
                    .debug_selector(|| "files-unshown".into())
                    .flex()
                    .flex_col()
                    .flex_1()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .gap(px(SPACE_2))
                    .px(px(SPACE_6))
                    .text_center()
                    .text_size(px(FS_LABEL))
                    .text_color(rgb(INK_SUBTLE))
                    .child(div().w_full().whitespace_normal().child(why.note().to_owned()))
                    .when(
                        matches!(why, Unshown::Binary(_)) && files.remote.is_none(),
                        |note| {
                            note.child(
                                div()
                                    .text_size(px(FS_META))
                                    .child("It has no text to show here."),
                            )
                            .child(
                                ferrite_ui::components::form_button_on("files-open-original", theme::paint::RAISED, cx)
                                    .debug_selector(|| "files-open-original".into())
                                    .h(px(ROW))
                                    .px(px(CH))
                                    .child(ferrite_ui::components::form_label("Open in default app", INK_STRONG))
                                    .on_click({
                                        let file = open_path.clone();
                                        cx.listener(move |_, _: &ClickEvent, window, cx| {
                                            open_original(&file, window, cx)
                                        })
                                    }),
                            )
                        },
                    )
                    .when(matches!(why, Unshown::Unreadable | Unshown::Remote(_)), |note| {
                        note.child(
                            ferrite_ui::components::form_button("files-retry", cx)
                                .debug_selector(|| "files-retry".into())
                                .h(px(ROW))
                                .px(px(CH))
                                .child(ferrite_ui::components::form_label("Try again", INK_SUBTLE))
                                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                                    if let Some(at) = view.files_ref(thread).and_then(|files| {
                                        files.tabs.iter().position(|tab| tab.path == retry_path)
                                    }) {
                                        view.reload_tab(thread, at, cx);
                                    }
                                })),
                        )
                    })
                    .into_any_element()
            }
        };
        column.child(breadcrumb).child(body)
    }

    fn files_tree(&self, thread: ThreadKey, files: &Files, cx: &mut Context<Self>) -> Div {
        use theme::*;
        let empty = files.filter.read(cx).text().is_empty();
        let field = div()
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(ICON_BUTTON))
            .mx(px(SPACE_2))
            .px(px(SPACE_2))
            .rounded(px(R_CONTROL))
            .bg(paint::FIELD)
            .on_action(cx.listener(move |view, _: &OpenFirst, _, cx| {
                view.open_first_match(thread, cx)
            }))
            .child(icon(icons::SEARCH, ICON_BUTTON_GLYPH, INK_SUBTLE))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(FS_LABEL))
                    .child(files.filter.clone())
                    .when(empty, |line| {
                        line.child(
                            div()
                                .debug_selector(|| "files-filter-placeholder".into())
                                .absolute()
                                .top_0()
                                .left_0()
                                .text_color(rgb(INK_SUBTLE))
                                .child("Filter, or ? to search in files"),
                        )
                    }),
            );
        let listing = files.tree.rows();
        let active = files.active_path().map(Path::to_path_buf);
        let body = div()
            .id("files-tree-scroll")
            .debug_selector(|| "files-tree".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&files.tree_scroll)
            .px(px(SPACE_2))
            .children(
                listing
                    .rows
                    .into_iter()
                    .enumerate()
                    .map(|(at, row)| self.tree_row(thread, at, row, active.as_deref(), cx)),
            )
            .children(listing.note.map(|note| {
                div()
                    .debug_selector(|| "files-tree-note".into())
                    .flex_shrink_0()
                    .p(px(SPACE_2))
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(note))
            }));
        div()
            .debug_selector(|| "files-tree-column".into())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h_full()
            .gap(px(SPACE_1))
            .child(div().h(px(ICON_BUTTON - SPACE_2)).flex_shrink_0())
            .child(field)
            .child(body)
    }

    fn tree_row(
        &self,
        thread: ThreadKey,
        at: usize,
        row: Row,
        active: Option<&Path>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        use theme::*;
        let path = row.entry.path.clone();
        let selected = active == Some(path.as_path());
        let ink = match row.status {
            Some(Status::Added) => GREEN,
            Some(Status::Modified) => YELLOW,
            Some(Status::Deleted) => RED,
            None => INK_STRONG,
        };
        let id = SharedString::from(format!("files-row-{at}"));
        let dir = row.entry.dir;
        let (press, menu_path) = (path.clone(), path);
        div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(FILES_ROW_H))
            .pl(px(SPACE_1 + row.depth as f32 * FILES_INDENT))
            .pr(px(SPACE_2))
            .rounded(px(R_CONTROL))
            .text_size(px(FS_LABEL))
            .text_color(rgb(ink))
            .whitespace_nowrap()
            .cursor_pointer()
            .when(selected, |row| row.bg(paint::SELECTED))
            .when(!selected, |row| row.hover(|row| row.bg(paint::RAISED)))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .justify_center()
                    .w(px(ICON_CHEVRON + SPACE_1))
                    .child(if dir {
                        ferrite_ui::components::chevron_at(
                            if row.expanded { 1. } else { 0. },
                            ICON_CHEVRON,
                            INK_SUBTLE,
                        )
                        .into_any_element()
                    } else {
                        icon(icons::CODE, ICON_CHEVRON, INK_SUBTLE).into_any_element()
                    }),
            )
            .child(div().flex_1().min_w_0().truncate().child(SharedString::from(row.entry.name)))
            .children(row.status.map(|_| {
                div()
                    .flex_shrink_0()
                    .size(px(FILES_STATUS_DOT))
                    .rounded_full()
                    .bg(rgb(ink))
            }))
            .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                let Some(files) = view.files_of(thread) else {
                    return;
                };
                if dir {
                    files.tree.toggle(&press);
                    if files.remote.is_some() {
                        view.refresh(thread, cx);
                    }
                    cx.notify();
                } else {
                    // A second press keeps the tab (`ClickEvent::click_count`).
                    let keep = event.click_count() >= 2;
                    cx.emit(FilesEvent::Open { thread, path: press.clone(), keep });
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if let Some(files) = view.files_of(thread) {
                        files.pressed = Some(menu_path.clone());
                    }
                    cx.emit(FilesEvent::Menu { thread, at: event.position });
                }),
            )
    }
}

impl Render for FilesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.cockpit.upgrade().and_then(|cockpit| cockpit.read(cx).files_inputs(cx)) {
            Some(inputs) => self.pane(&inputs, cx),
            None => div().flex_1(),
        }
    }
}

#[cfg(test)]
impl FilesView {
    /// `thread`'s Files in `root` on this computer, with no tabs kept.
    pub(in crate::cockpit) fn make_for_test(&mut self, thread: ThreadKey, root: PathBuf, cx: &mut Context<Self>) {
        self.make(thread, root, None, None, cx);
    }

    fn remote_for_test(
        &mut self,
        thread: ThreadKey,
        answer: impl Fn(ferrite_core::file_operations::Request) -> std::io::Result<FileReply> + 'static,
    ) {
        if let Some(files) = self.files_of(thread) {
            let root = files.tree.root().to_path_buf();
            files.tree = FileTree::remote(root.clone());
            files.remote = Some(RemoteFiles::fake(root, answer));
            files.tabs.clear();
            files.active = None;
        }
    }

    /// What the shown rendered file's bar reads, and where its occurrence painted.
    pub(crate) fn prose_find_for_test(
        &mut self,
        thread: ThreadKey,
    ) -> Option<(Option<SharedString>, Option<gpui::Bounds<Pixels>>)> {
        let (_, rendered) = self.shown_rendered(thread)?;
        rendered.find.as_ref().map(PreviewFind::state)
    }

    /// Each open tab's name and whether it is a preview tab.
    pub(in crate::cockpit) fn tab_names(&self, thread: ThreadKey) -> Vec<(String, bool)> {
        self.files_ref(thread)
            .map(|files| {
                files
                    .tabs
                    .iter()
                    .map(|tab| {
                        let name = tab.path.file_name().unwrap().to_string_lossy().into_owned();
                        (name, tab.preview)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(in crate::cockpit) fn wraps(&self, thread: ThreadKey) -> Option<bool> {
        self.files_ref(thread).map(|files| files.settings.wrap)
    }

    pub(in crate::cockpit) fn row_names(&self, thread: ThreadKey) -> Vec<String> {
        self.files_ref(thread)
            .map(|files| files.tree.rows().rows.into_iter().map(|row| row.entry.name).collect())
            .unwrap_or_default()
    }

    pub(in crate::cockpit) fn rendered_markup(&self, thread: ThreadKey) -> Option<String> {
        let files = self.files_ref(thread)?;
        match &files.tabs[files.active?].reading {
            Reading::Rendered(rendered) => Some(rendered.markup.to_string()),
            _ => None,
        }
    }

    pub(in crate::cockpit) fn unshown_note(&self, thread: ThreadKey) -> Option<String> {
        let files = self.files_ref(thread)?;
        match &files.tabs[files.active?].body {
            TabBody::Unshown(why) => Some(why.note().to_string()),
            _ => None,
        }
    }

    /// The path of the shown tab.
    pub(in crate::cockpit) fn active_path(&self, thread: ThreadKey) -> Option<PathBuf> {
        self.files_ref(thread)?.active_path().map(Path::to_path_buf)
    }

    pub(in crate::cockpit) fn status_for_test(&self, thread: ThreadKey, path: &Path) -> Option<Status> {
        self.files_ref(thread)?.tree.status_of(path)
    }

    pub(in crate::cockpit) fn filter_text_for_test(&self, thread: ThreadKey, cx: &gpui::App) -> String {
        self.files_ref(thread)
            .map_or_else(String::new, |files| files.filter.read(cx).text().to_string())
    }

    pub(in crate::cockpit) fn set_filter_for_test(&mut self, thread: ThreadKey, text: &str, cx: &mut Context<Self>) {
        if let Some(files) = self.files_ref(thread) {
            let field = files.filter.clone();
            field.update(cx, |field, cx| field.set(text.to_string(), cx));
        }
    }
}
