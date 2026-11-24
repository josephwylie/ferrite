//! A Machine's folders browsed from this computer (ADR 0026), where no
//! folder dialog can reach. The line names the folder shown; under it its
//! folders, listed by the Machine's Server (`list_dir`), each a press away
//! from going into it, and `..` out of it. The arrows walk the rows and ↵
//! goes into the one they rest on. The line can still be typed into: its
//! last name narrows the rows. The setup sheet's destination and the
//! Project card's directory on a Machine are each one.

use super::*;
use ferrite_core::folder::Listing;
use gpui::App;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

gpui::actions!(folder_browser, [Up, Down, Open, Cancel]);

/// The key context of the browser's lines (`ferrite_ui::keymap`).
const KEY_CONTEXT: &str = "FolderBrowser";

/// How many rows the list shows, however many the folder has, so the
/// sheet holds still as the browser moves: more scroll.
const ROWS: f32 = 6.0;

/// Where the browser goes when the folder it was put at cannot be read.
const HOME: &str = "~/";

const SEPARATORS: [char; 2] = ['/', '\\'];

pub(super) struct FolderBrowser {
    model: Entity<Cockpit>,
    machine: MachineId,
    label: SharedString,
    /// The folder shown, `/` last; a last name without one narrows its
    /// rows.
    pub(super) field: Entity<Composer>,
    /// The directory the rows are of, as the line has it.
    shown: String,
    /// Each directory read: what is in it, or why it could not be read.
    listed: HashMap<String, Result<Listing, SharedString>>,
    /// Directories asked for and not answered yet.
    asking: Vec<(String, Receiver<std::io::Result<Listing>>)>,
    /// The row the arrows rest on.
    cursor: Option<usize>,
    /// The folder the browser was put at, while the line still holds it.
    put: Option<String>,
    /// Folders named here that are not on the Machine yet: what goes into
    /// one makes it.
    made: HashSet<String>,
    /// A new folder's name, while one is being named.
    naming: Option<Entity<Composer>>,
    scroll: ScrollHandle,
    /// The folder shown is held, while a copy goes into it: the line takes
    /// no keys and the rows no presses, all drawn quiet.
    held: bool,
}

/// The line moved: to another folder, or by a keystroke.
pub(super) struct Moved;

impl EventEmitter<Moved> for FolderBrowser {}

/// One row under the line.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    /// Out of the folder shown, into this one.
    Up(String),
    Folder(String),
}

/// A typed path's directory and the start of the name in it: `~/src/fe` is
/// `~/src/` and `fe`.
fn split_typed(typed: &str) -> (&str, &str) {
    match typed.rfind(SEPARATORS) {
        Some(at) => typed.split_at(at + 1),
        None => ("", typed),
    }
}

/// The folder `dir` is in: `~/src/` is in `~/`, `/home/` in `/`, `~/` in
/// the folder its listing spells home out as. None at a root.
fn parent_of(dir: &str, listing: Option<&Listing>) -> Option<String> {
    let spelled;
    let mut trimmed = dir.trim_end_matches(SEPARATORS);
    if trimmed == "~" {
        spelled = listing?.path.display().to_string();
        trimmed = spelled.trim_end_matches(SEPARATORS);
    }
    let at = trimmed.rfind(SEPARATORS)?;
    Some(format!("{}/", &trimmed[..at]))
}

/// `name` in the Machine's folder `parent`, joined with `/` as every path
/// here is: `Path::join` would join with this computer's separator.
fn child(parent: &Path, name: &str) -> PathBuf {
    let parent = parent.display().to_string();
    format!("{}/{name}", parent.trim_end_matches(SEPARATORS)).into()
}

impl FolderBrowser {
    /// A browser of `machine`'s folders labelled `label`, opened at
    /// `start`, or at home when `start` cannot be read.
    pub(super) fn new(
        model: Entity<Cockpit>,
        machine: MachineId,
        label: SharedString,
        start: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = Self::line(cx);
        field.update(cx, |field, cx| field.set(start.clone(), cx));
        cx.subscribe(&field, |browser: &mut Self, _, _: &Edited, cx| {
            browser.edited(cx)
        })
        .detach();
        let mut browser = Self {
            model,
            machine,
            label,
            field,
            shown: String::new(),
            listed: HashMap::new(),
            asking: Vec::new(),
            cursor: None,
            put: Some(start),
            made: HashSet::new(),
            naming: None,
            scroll: ScrollHandle::new(),
            held: false,
        };
        browser.edited(cx);
        browser
    }

    /// Hold the folder shown, or let it go.
    pub(super) fn hold(&mut self, held: bool, cx: &mut Context<Self>) {
        if self.held == held {
            return;
        }
        self.held = held;
        self.naming = None;
        self.cursor = None;
        cx.notify();
    }

    fn line(cx: &mut Context<Self>) -> Entity<Composer> {
        let line = cx.new(Composer::new);
        line.update(cx, |line, cx| {
            line.set_role(ferrite_pane::composer::Role::Field(Some(KEY_CONTEXT)), cx)
        });
        line
    }

    fn typed<'a>(&self, cx: &'a App) -> &'a str {
        self.field.read(cx).text()
    }

    /// The folder the line names: what is typed, no `/` last but a root's.
    pub(super) fn folder(&self, cx: &App) -> String {
        let typed = self.typed(cx).trim();
        match typed.trim_end_matches(SEPARATORS) {
            "" if !typed.is_empty() => typed[..1].to_string(),
            folder => folder.to_string(),
        }
    }

    pub(super) fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.read(cx).focus_handle(cx)
    }

    /// Whether the browser holds the keyboard: its line, or a new folder's
    /// name.
    pub(super) fn focused(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle(cx).is_focused(window) || self.naming_focused(window, cx)
    }

    fn naming_focused(&self, window: &Window, cx: &App) -> bool {
        self.naming
            .as_ref()
            .is_some_and(|naming| naming.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Show `folder`, `/` last, unless the folder shown is held.
    fn go(&mut self, folder: String, cx: &mut Context<Self>) {
        if !self.held {
            self.field.update(cx, |field, cx| field.set(folder, cx));
        }
    }

    /// Show `folder`, as the browser's own move rather than the operator's.
    fn put_at(&mut self, folder: String, cx: &mut Context<Self>) {
        self.put = Some(folder.clone());
        self.go(folder, cx);
    }

    /// Whether the line still holds the folder the browser was put at:
    /// nothing has been browsed to since.
    pub(super) fn untouched(&self) -> bool {
        self.put.is_some()
    }

    /// The folder shown, spelled out, for its owner to keep; the browser
    /// goes back to the folder it is in. None when the line names none.
    pub(super) fn take(&mut self, cx: &mut Context<Self>) -> Option<PathBuf> {
        let folder = self.folder(cx);
        if folder.is_empty() {
            return None;
        }
        let path = self.spelled_out(&folder);
        let listing = self
            .listed
            .get(&format!("{}/", folder.trim_end_matches(SEPARATORS)))
            .and_then(|listed| listed.as_ref().ok());
        let back = parent_of(&folder, listing).unwrap_or(folder);
        self.put_at(back, cx);
        Some(path)
    }

    /// The line moved. Into another directory: its rows are read, one that
    /// could not be read is asked again.
    fn edited(&mut self, cx: &mut Context<Self>) {
        self.cursor = None;
        let typed = self.typed(cx).to_string();
        if self.put.as_deref() != Some(typed.as_str()) {
            self.put = None;
        }
        let (directory, _) = split_typed(&typed);
        if directory != self.shown {
            self.shown = directory.to_string();
            if self.listed.get(directory).is_some_and(Result::is_err) {
                self.listed.remove(directory);
            }
            self.scroll.set_offset(gpui::Point::default());
        }
        self.list(cx);
        cx.emit(Moved);
        cx.notify();
    }

    /// Ask the Machine for the directory the rows are of, unless it is read
    /// or on its way, or was named here and is not made yet.
    fn list(&mut self, cx: &mut Context<Self>) {
        let directory = self.shown.clone();
        if directory.is_empty()
            || self.made.contains(&directory)
            || self.listed.contains_key(&directory)
            || self.asking.iter().any(|(asked, _)| *asked == directory)
        {
            return;
        }
        let machine = self.machine;
        let path = PathBuf::from(&directory);
        if let Ok(answer) = self
            .model
            .update(cx, |cockpit, _| cockpit.list_dir(machine, &path))
        {
            self.asking.push((directory, answer));
        }
    }

    /// Take the Machine's answers, once they land. A browser put at a
    /// folder that cannot be read goes home; an answer the lost connection
    /// took is asked again at the next move.
    pub(super) fn poll(&mut self, cx: &mut Context<Self>) {
        let mut landed = false;
        let listed = &mut self.listed;
        self.asking.retain(|(directory, answer)| {
            let listing = match answer.try_recv() {
                Err(TryRecvError::Empty) => return true,
                Ok(Ok(listing)) => Ok(listing),
                Ok(Err(error)) => Err(SharedString::from(error.to_string())),
                Err(TryRecvError::Disconnected) => return false,
            };
            listed.insert(directory.clone(), listing);
            landed = true;
            false
        });
        if !landed {
            return;
        }
        let put_unread = self.put.as_ref().is_some_and(|put| {
            let (directory, _) = split_typed(put);
            self.listed.get(directory).is_some_and(Result::is_err)
        });
        if put_unread && self.put.as_deref() != Some(HOME) {
            self.put_at(HOME.into(), cx);
        }
        cx.notify();
    }

    /// `path` as the Machine's Server spelled it out, its `~` the Machine's
    /// home: by the folder's own listing, or its parent's. As typed
    /// otherwise: the Server spells `~` out when it registers.
    pub(super) fn spelled_out(&self, path: &str) -> PathBuf {
        let listing = |directory: &str| {
            self.listed
                .get(directory)
                .and_then(|listed| listed.as_ref().ok())
                .map(|listing| listing.path.clone())
        };
        let trimmed = path.trim_end_matches(SEPARATORS);
        if let Some(path) = listing(&format!("{trimmed}/")) {
            return path;
        }
        let (directory, leaf) = split_typed(trimmed);
        listing(directory).map_or_else(|| path.into(), |parent| child(&parent, leaf))
    }

    /// The rows under the line: `..`, then the folders whose names start as
    /// its last name does, a dot folder only once a dot is typed.
    fn rows(&self, cx: &App) -> Vec<Row> {
        let (directory, start) = split_typed(self.typed(cx));
        if directory.is_empty() {
            return Vec::new();
        }
        let listing = self
            .listed
            .get(directory)
            .and_then(|listed| listed.as_ref().ok());
        let up = parent_of(directory, listing).map(Row::Up);
        let folders = listing.into_iter().flat_map(|listing| {
            listing
                .entries
                .iter()
                .filter(|entry| {
                    entry.directory
                        && entry.name.starts_with(start)
                        && (start.starts_with('.') || !entry.name.starts_with('.'))
                })
                .map(|entry| Row::Folder(entry.name.clone()))
        });
        up.into_iter().chain(folders).collect()
    }

    fn open(&mut self, row: Row, cx: &mut Context<Self>) {
        let folder = match row {
            Row::Up(parent) => parent,
            Row::Folder(name) => format!("{}{name}/", self.shown),
        };
        self.go(folder, cx);
    }

    /// Tab in the line: go into the first folder its last name fits.
    /// False when the browser does not hold the keyboard.
    pub(super) fn tab(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if !self.focused(window, cx) {
            return false;
        }
        let first = self
            .rows(cx)
            .into_iter()
            .find(|row| matches!(row, Row::Folder(_)));
        if let Some(first) = first {
            self.open(first, cx);
        }
        true
    }

    fn step(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.naming_focused(window, cx) {
            return cx.propagate();
        }
        let rows = self.rows(cx).len();
        if rows == 0 || self.held {
            return;
        }
        let last = rows as isize - 1;
        let at = match self.cursor {
            None if by > 0 => 0,
            None => last,
            Some(at) => (at as isize + by).clamp(0, last),
        } as usize;
        self.cursor = Some(at);
        self.scroll.scroll_to_item(at);
        cx.notify();
    }

    /// ↵: make the folder being named, or go into the row the arrows rest
    /// on. With neither it is the owner's.
    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.naming_focused(window, cx) {
            return self.finish_naming(window, cx);
        }
        let row = self.cursor.and_then(|at| self.rows(cx).into_iter().nth(at));
        match row {
            Some(row) => self.open(row, cx),
            None => cx.propagate(),
        }
    }

    /// esc: leave the folder unnamed, or the rows. With neither it is the
    /// owner's.
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.naming.take().is_some() {
            window.focus(&self.focus_handle(cx), cx);
        } else if self.cursor.take().is_none() {
            return cx.propagate();
        }
        cx.notify();
    }

    /// Name a folder in the one shown, made by what goes into it: a row
    /// under its folders.
    pub(super) fn new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.held {
            return;
        }
        let naming = match &self.naming {
            Some(naming) => naming.clone(),
            None => Self::line(cx),
        };
        self.naming = Some(naming.clone());
        window.focus(&naming.read(cx).focus_handle(cx), cx);
        self.cursor = None;
        self.scroll.scroll_to_item(self.rows(cx).len());
        cx.notify();
    }

    fn finish_naming(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(naming) = self.naming.take() else {
            return;
        };
        let name = naming
            .read(cx)
            .text()
            .trim()
            .trim_matches(SEPARATORS)
            .to_string();
        window.focus(&self.focus_handle(cx), cx);
        if name.is_empty() {
            return cx.notify();
        }
        let folder = format!("{}{name}/", self.shown);
        self.made.insert(folder.clone());
        self.go(folder, cx);
    }

    /// Under the rows, in the list: the folder on its way, why it cannot be
    /// read, that it is new, or that nothing is in it.
    fn note(&self, rows: usize, cx: &App) -> Option<Div> {
        let (directory, start) = split_typed(self.typed(cx));
        if directory.is_empty() {
            return None;
        }
        let quiet = |text: String| {
            ferrite_ui::components::text_ui()
                .flex_shrink_0()
                .text_color(gpui::rgb(ferrite_ui::theme::INK_SUBTLE))
                .child(text)
        };
        if self.made.contains(directory) {
            return Some(quiet("New folder".into()).debug_selector(|| "folder-browser-new".into()));
        }
        match self.listed.get(directory) {
            None => Some(
                quiet(format!("Reading {directory}"))
                    .debug_selector(|| "folder-browser-reading".into()),
            ),
            Some(Err(error)) => Some(
                project_editor::error_line(
                    format!("{directory} cannot be read \u{b7} {error}").into(),
                )
                .debug_selector(|| "folder-browser-unread".into()),
            ),
            Some(Ok(_)) if start.is_empty() && rows <= 1 => {
                Some(quiet("No folders".into()).debug_selector(|| "folder-browser-empty".into()))
            }
            Some(Ok(_)) => None,
        }
    }
}

impl Render for FolderBrowser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme::*;
        let rows = self.rows(cx);
        let note = self.note(rows.len(), cx);
        let drawn: Vec<_> = rows
            .into_iter()
            .enumerate()
            .map(|(at, row)| {
                let (icon, label, selector) = match &row {
                    Row::Up(_) => (
                        ferrite_ui::icons::CORNER_LEFT_UP,
                        "..".to_string(),
                        "folder-row-up".to_string(),
                    ),
                    Row::Folder(name) => (
                        ferrite_ui::icons::FOLDER,
                        name.clone(),
                        format!("folder-row:{name}"),
                    ),
                };
                project_editor::folder_row(
                    ("folder-row", at),
                    icon,
                    label.into(),
                    self.cursor == Some(at),
                    self.held,
                    cx,
                )
                .debug_selector(move || selector.clone())
                .on_click(cx.listener(
                    move |browser, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        window.focus(&browser.focus_handle(cx), cx);
                        browser.open(row.clone(), cx);
                    },
                ))
            })
            .collect();
        let line = if self.held {
            ferrite_ui::components::text_ui()
                .truncate()
                .text_color(gpui::rgb(INK_SUBTLE))
                .child(self.typed(cx).to_string())
                .into_any_element()
        } else {
            self.field.clone().into_any_element()
        };
        let naming = self.naming.clone().map(|naming| {
            let focused = naming.read(cx).focus_handle(cx).is_focused(window);
            let empty = naming.read(cx).text().is_empty();
            project_editor::folder_name_row(naming, focused, empty)
                .debug_selector(|| "folder-browser-naming".into())
        });
        div()
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(|browser, _: &Up, window, cx| browser.step(-1, window, cx)))
            .on_action(cx.listener(|browser, _: &Down, window, cx| browser.step(1, window, cx)))
            .on_action(cx.listener(|browser, _: &Open, window, cx| browser.confirm(window, cx)))
            .on_action(cx.listener(|browser, _: &Cancel, window, cx| browser.cancel(window, cx)))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .child(project_editor::path_field(
                self.label.clone(),
                line,
                !self.held && self.focus_handle(cx).is_focused(window),
            ))
            .child(
                div()
                    .id("folder-browser-rows")
                    .debug_selector(|| "folder-browser-rows".into())
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .h(px(ROWS * FILES_ROW_H))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(drawn)
                    .children(naming)
                    .children(note.map(project_editor::folder_note)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folders_parent_is_the_folder_it_is_in() {
        assert_eq!(parent_of("~/src/", None).as_deref(), Some("~/"));
        assert_eq!(parent_of("/home/", None).as_deref(), Some("/"));
        assert_eq!(parent_of("/", None), None);
        assert_eq!(parent_of(r"D:\work\", None).as_deref(), Some("D:/"));
        assert_eq!(parent_of(r"D:\", None), None);
        assert_eq!(parent_of("~/", None), None, "home waits for its listing");
        let home = Listing {
            path: "/home/me".into(),
            entries: Vec::new(),
        };
        assert_eq!(parent_of("~/", Some(&home)).as_deref(), Some("/home/"));
    }

    #[test]
    fn a_folder_in_a_machines_folder_is_joined_with_a_slash() {
        let joined =
            |parent: &str, name: &str| child(Path::new(parent), name).display().to_string();
        assert_eq!(joined("/home/me/src", "repo"), "/home/me/src/repo");
        assert_eq!(joined("/", "srv"), "/srv");
        assert_eq!(joined(r"D:\work\", "repo"), r"D:\work/repo");
    }
}
