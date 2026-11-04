//! Files (Claude's explorer, `files-empty.png`, `files-code.png`,
//! `files-more-menu.png`): the dock's two-column pane — the open files'
//! editor at the left, the checkout's tree at the right.
//!
//! - **Head:** the editor's tab strip (a preview tab, one a press opened
//!   and the next press replaces, reads in italic; typing or a second
//!   press keeps it), then `Go to file` (⌘P, the tree's filter), `Collapse
//!   all folders`, `Hide file tree` (⇧⌘Y), `⋯`, expand and close.
//! - **Tree** (`tree`): a filter field, folders that open on a press, git's
//!   colour on each name; a right-click offers `Attach as context`, `Copy ›`
//!   and `Show in Finder`.
//! - **Editor** (`editor`): a code file edits, with Auto save; prose (a
//!   `.md` or `.txt`) is read as the reader reads it — the reading face,
//!   Markdown rendered — not in the code face. A file that is no text
//!   says so.
//! - **Empty:** `Open files appear here`.
//! - **`⋯`:** Show file tree, Preview tabs, Auto save, Word wrap, Tab size.
//!
//! Every way a file opens in the right pane — a path in the conversation,
//! `Open in Files`, the tree — arrives at `open_in_files`.

use std::path::{Path, PathBuf};

use ferrite_pane::composer::{Composer, Role};
use ferrite_pane::preview::{head_door, Body as DocBody, Document, Text};
use ferrite_ui::icons::{self, icon};
use ferrite_ui::theme::{self};

use super::editor::{self, Editor};
use super::tree::{FileTree, Row, Status};
use super::*;
use crate::cockpit::view_head::Column;
use crate::cockpit::context_menu::Submenu;

/// The filter field's key context: ⏎ opens the first match.
pub(crate) const FILTER_CONTEXT: &str = "FilesFilter";
/// The most rows a filter's list draws.
const FILTER_ROWS: usize = 200;

gpui::actions!(files, [OpenFirst]);

/// What the Files ⋯ menu and the head's doors do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verb {
    ToggleTree,
    CollapseAll,
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
    /// Prose, read as the reader reads it.
    Prose {
        document: Document,
        rich: ferrite_pane::rich::TextCache,
        scroll: ScrollHandle,
    },
    Unshown(&'static str),
}

struct Tab {
    path: PathBuf,
    /// A preview tab: the next open replaces it, until it is edited or
    /// opened twice.
    preview: bool,
    body: TabBody,
}

pub(crate) struct Files {
    tree: FileTree,
    filter: Entity<Composer>,
    tabs: Vec<Tab>,
    active: Option<usize>,
    settings: editor::Settings,
    preview_tabs: bool,
    show_tree: bool,
    tree_scroll: ScrollHandle,
    /// The tree row a right-click is on, for the menu's verbs.
    pressed: Option<PathBuf>,
}

impl Files {
    fn active_path(&self) -> Option<&Path> {
        self.active.map(|at| self.tabs[at].path.as_path())
    }
}

impl CockpitView {
    fn files_of(&mut self, thread: ThreadId) -> Option<&mut Files> {
        self.tools.files.get_mut(&thread)
    }

    pub(in crate::cockpit) fn files_ref(&self, thread: ThreadId) -> Option<&Files> {
        self.tools.files.get(&thread)
    }

    /// The Files state for `thread`, made the first time it is asked for.
    pub(in crate::cockpit) fn ensure_files(
        &mut self,
        thread: ThreadId,
        cx: &mut Context<Self>,
    ) -> Option<&mut Files> {
        if !self.tools.files.contains_key(&thread) {
            let root = self.thread_path(thread)?;
            let filter = cx.new(|cx| {
                let mut field = Composer::new(cx);
                field.set_role(Role::Field(Some(FILTER_CONTEXT)), cx);
                field
            });
            cx.subscribe(&filter, move |view, field, _: &ferrite_pane::composer::Edited, cx| {
                let text = field.read(cx).text().to_string();
                if let Some(files) = view.files_of(thread) {
                    files.tree.set_filter(text);
                    cx.notify();
                }
            })
            .detach();
            self.tools.files.insert(
                thread,
                Files {
                    tree: FileTree::new(root),
                    filter,
                    tabs: Vec::new(),
                    active: None,
                    settings: editor::Settings::default(),
                    preview_tabs: true,
                    show_tree: true,
                    tree_scroll: ScrollHandle::new(),
                    pressed: None,
                },
            );
        }
        self.files_of(thread)
    }

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
        self.show_files(thread, cx);
        let Some(files) = self.ensure_files(thread, cx) else {
            return;
        };
        files.tree.reveal(&path);
        if let Some(at) = files.tabs.iter().position(|tab| tab.path == path) {
            files.active = Some(at);
            files.tabs[at].preview &= !keep;
        } else {
            let preview = files.preview_tabs && !keep;
            let settings = files.settings;
            let Some(body) = Self::tab_body(&path, settings, thread, cx) else {
                return;
            };
            let Some(files) = self.files_of(thread) else {
                return;
            };
            if preview {
                if let Some(at) = files.tabs.iter().position(|tab| tab.preview) {
                    files.tabs.remove(at);
                }
            }
            files.tabs.push(Tab { path: path.clone(), preview, body });
            files.active = Some(files.tabs.len() - 1);
        }
        if let (Some(line), Some(Tab { body: TabBody::Code(editor), .. })) = (
            line,
            self.files_of(thread).and_then(|files| files.active.map(|at| &files.tabs[at])),
        ) {
            editor.update(cx, |editor, cx| editor.go_to_line(line as usize, cx));
        }
        cx.notify();
    }

    fn tab_body(
        path: &Path,
        settings: editor::Settings,
        thread: ThreadId,
        cx: &mut Context<Self>,
    ) -> Option<TabBody> {
        let Ok(bytes) = std::fs::read(path) else {
            return Some(TabBody::Unshown("Could not read this file"));
        };
        let Ok(source) = String::from_utf8(bytes) else {
            return Some(TabBody::Unshown("This is not a text file"));
        };
        let document = Document {
            path: path.to_path_buf(),
            name: None,
            body: DocBody::Text {
                text: std::sync::Arc::new(Text::new(source.clone())),
                line: None,
            },
        };
        if document.rich_source().is_some() {
            return Some(TabBody::Prose {
                document,
                rich: Default::default(),
                scroll: ScrollHandle::new(),
            });
        }
        let editor = cx.new(|cx| Editor::new(path, source, settings, cx));
        cx.subscribe(&editor, move |view, editor, _: &editor::Edited, cx| {
            // Typing keeps a preview tab.
            if let Some(files) = view.files_of(thread) {
                for tab in &mut files.tabs {
                    if matches!(&tab.body, TabBody::Code(own) if *own == editor) {
                        tab.preview = false;
                    }
                }
            }
            cx.notify();
        })
        .detach();
        Some(TabBody::Code(editor))
    }

    /// Show the Files pane for `thread` (the Files door, opened).
    fn show_files(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if !self.tools.dock(thread).files {
            let dock = self.tools.docks.entry(thread).or_default();
            dock.files = true;
            self.give_column_to_tools(thread, cx);
        }
    }

    fn select_tab(&mut self, thread: ThreadId, at: usize, cx: &mut Context<Self>) {
        if let Some(files) = self.files_of(thread) {
            files.active = Some(at);
            cx.notify();
        }
    }

    fn close_tab(&mut self, thread: ThreadId, at: usize, cx: &mut Context<Self>) {
        let Some(files) = self.files_of(thread) else {
            return;
        };
        let tab = files.tabs.remove(at);
        if let TabBody::Code(editor) = &tab.body {
            editor.update(cx, |editor, _| editor.save());
        }
        files.active = match files.tabs.len() {
            0 => None,
            len => Some(files.active.map_or(0, |active| active.min(len - 1))),
        };
        cx.notify();
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
                    && self.thread_path(thread).is_some_and(|root| document.path.starts_with(root));
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

    /// ⌘P: the tree's filter takes the keyboard.
    pub(in crate::cockpit) fn go_to_file(&mut self, thread: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        self.show_files(thread, cx);
        if let Some(files) = self.ensure_files(thread, cx) {
            files.show_tree = true;
            let focus = files.filter.read(cx).focus_target(window, cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn open_first_match(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let first = self.files_of(thread).and_then(|files| {
            files.tree.rows().into_iter().find(|row| !row.entry.dir).map(|row| row.entry.path)
        });
        if let Some(path) = first {
            self.open_in_files(thread, path, None, false, cx);
        }
    }

    pub(in crate::cockpit) fn toggle_file_tree(&mut self, cx: &mut Context<Self>) {
        if let Some(files) = self.focused_thread().and_then(|thread| self.files_of(thread)) {
            files.show_tree = !files.show_tree;
            cx.notify();
        }
    }

    /// A Files menu verb (the ⋯ menu's rows, a tree row's menu).
    pub(in crate::cockpit) fn run_files_verb(&mut self, thread: ThreadId, verb: Verb, cx: &mut Context<Self>) {
        let Some(files) = self.files_of(thread) else {
            return;
        };
        let pressed = files.pressed.clone();
        match verb {
            Verb::ToggleTree => files.show_tree = !files.show_tree,
            Verb::CollapseAll => files.tree.collapse_all(),
            Verb::TogglePreviewTabs => files.preview_tabs = !files.preview_tabs,
            Verb::ToggleAutoSave => files.settings.auto_save = !files.settings.auto_save,
            Verb::ToggleWrap => files.settings.wrap = !files.settings.wrap,
            Verb::TabSize(size) => files.settings.tab_size = size,
            Verb::Attach => {
                if let Some(path) = pressed {
                    if let Some(index) = self.pane_for(thread) {
                        self.panes[index]
                            .composer
                            .update(cx, |composer, cx| composer.add_files(&[path], cx));
                    }
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
    pub(in crate::cockpit) fn files_pressed(&self, thread: ThreadId) -> Option<PathBuf> {
        self.files_ref(thread)?.pressed.clone()
    }

    /// The rows of the Files ⋯ menu.
    pub(in crate::cockpit) fn files_more_rows(&self, thread: ThreadId) -> Vec<Option<(menu::Item, MenuVerb)>> {
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

    pub(in crate::cockpit) fn files_sub_rows(&self, thread: ThreadId, sub: Sub) -> Vec<(menu::Item, MenuVerb)> {
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

    /// The tree row menu's rows.
    pub(in crate::cockpit) fn files_row_rows(&self) -> Vec<Option<(menu::Item, MenuVerb)>> {
        vec![
            Some((menu::Item::new("Attach as context"), MenuVerb::Files(Verb::Attach))),
            Some((menu::Item::new("Copy"), MenuVerb::Submenu(Submenu::Files(Sub::Copy)))),
            None,
            Some((menu::Item::new("Show in Finder"), MenuVerb::Reveal)),
        ]
    }

    // ------------------------------------------------------------ drawing

    /// The Files pane: its head over the editor and the tree.
    pub(in crate::cockpit) fn files_pane(&self, thread: ThreadId, cx: &mut Context<Self>) -> Div {
        let Some(files) = self.files_ref(thread) else {
            return div().flex_1();
        };
        div()
            .debug_selector(|| "files".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.files_head(thread, files, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.files_editor(thread, files, cx))
                    .children(files.show_tree.then(|| self.files_tree(thread, files, cx))),
            )
    }

    fn files_head(&self, thread: ThreadId, files: &Files, cx: &mut Context<Self>) -> Div {
        use theme::*;
        let tabs = files.tabs.iter().enumerate().map(|(at, tab)| {
            let active = files.active == Some(at);
            let dirty = matches!(&tab.body, TabBody::Code(editor) if editor.read(cx).is_dirty());
            let name = tab
                .path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let id = SharedString::from(format!("files-tab-{at}"));
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_1))
                .h(px(PANEL_TAB_H))
                .pl(px(PANEL_TAB_PAD_X))
                .pr(px(SPACE_1))
                .max_w(px(PANEL_PREVIEW_TAB_MAX_W))
                .rounded(px(R_CONTROL))
                .text_size(px(FS_LABEL))
                .text_color(rgb(if active { INK_STRONG } else { INK_SUBTLE }))
                .when(active, |tab| tab.bg(paint::SELECTED))
                .when(tab.preview, |tab| tab.italic())
                .cursor_pointer()
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.select_tab(thread, at, cx)))
                .child(div().min_w_0().truncate().child(SharedString::from(name)))
                .child(
                    {
                        // This tab's own close: its key names the tab.
                        let key = SharedString::from(format!("files-tab-close-{at}"));
                        let selector = key.to_string();
                        div()
                            .id(key.clone())
                            .debug_selector(move || selector.clone())
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .size(px(PANEL_TAB_H - SPACE_1))
                            .rounded(px(R_CONTROL))
                            .hover_control(key)
                            .child(icon(
                                if dirty { icons::CIRCLE_DASHED } else { icons::CLOSE },
                                ICON_BUTTON_GLYPH,
                                INK_SUBTLE,
                            ))
                    }
                    .tooltip(ferrite_ui::menu::tooltip("Close"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.close_tab(thread, at, cx);
                    })),
                )
        });
        let door = |key: &'static str, glyph: &'static str, tip: &'static str, chord: Option<&'static str>| {
            let door = head_door(key, key.into(), glyph);
            match chord {
                Some(action) => door.tooltip(ferrite_ui::menu::action_tooltip(tip, action)),
                None => door.tooltip(ferrite_ui::menu::tooltip(tip)),
            }
        };
        let expanded = self.panel.expanded;
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(WIN_CHROME_H))
            .px(px(SIDE_CHAT_INSET))
            .pr(px(super::super::panel::tools_inset() - PANEL_PAD_R))
            .mt(px(self.head_drop(Column::Files)))
            .child(
                div()
                    .debug_selector(|| "files-tabs".into())
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(SPACE_0_5))
                    .overflow_hidden()
                    .when(files.tabs.is_empty(), |strip| {
                        strip.text_size(px(FS_LABEL)).text_color(rgb(INK_SUBTLE)).child("Files")
                    })
                    .children(tabs),
            )
            .child(
                door("files-go", icons::SEARCH, "Go to file or search", Some("cockpit::GoToFile"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        view.go_to_file(thread, window, cx);
                    })),
            )
            .child(
                door("files-collapse", icons::COLLAPSE_ALL, "Collapse all folders", None)
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.run_files_verb(thread, Verb::CollapseAll, cx);
                    })),
            )
            .child(
                door("files-tree-toggle", icons::LIST, "Hide file tree", Some("cockpit::ToggleFileTree"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.run_files_verb(thread, Verb::ToggleTree, cx);
                    })),
            )
            .child(
                door("files-more", icons::ELLIPSIS, "More", None).on_click(cx.listener(
                    move |view, event: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.open_context_menu(MenuTarget::FilesMore(thread), event.position(), cx);
                    },
                )),
            )
            .child(
                door(
                    "files-expand",
                    if expanded { icons::MINIMIZE } else { icons::MAXIMIZE },
                    if expanded { "Collapse" } else { "Expand" },
                    None,
                )
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.panel.expanded = !view.panel.expanded;
                    cx.notify();
                })),
            )
            .child(
                door("files-close", icons::CLOSE, "Close", None).on_click(cx.listener(
                    move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.press_tool(Tool::Files, cx);
                    },
                )),
            )
    }

    fn files_editor(&self, _thread: ThreadId, files: &Files, _cx: &mut Context<Self>) -> Div {
        use theme::*;
        let column = div()
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
                    .items_center()
                    .justify_center()
                    .gap(px(SPACE_2))
                    .px(px(SPACE_6))
                    .text_center()
                    .child(icon(icons::FOLDER, ICON_BUTTON_GLYPH + 4., INK_SUBTLE))
                    .child(div().text_size(px(FS_LABEL)).child("Open files appear here"))
                    .child(
                        div()
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_SUBTLE))
                            .child("Pick a file in the tree, or click a file path in the conversation."),
                    ),
            );
        };
        let relative = tab.path.strip_prefix(files.tree.root()).unwrap_or(&tab.path);
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
            .children(crumbs.into_iter().enumerate().flat_map(|(at, part)| {
                [
                    (at > 0).then(|| div().child("/").into_any_element()),
                    Some(div().child(SharedString::from(part)).into_any_element()),
                ]
            }).flatten());
        let body = match &tab.body {
            TabBody::Code(editor) => div().flex_1().min_h_0().child(editor.clone()).into_any_element(),
            TabBody::Prose { document, rich, scroll } => div()
                .id("files-prose")
                .debug_selector(|| "files-prose".into())
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .px(px(TX_INSET_SOLO))
                .pt(px(BODY_PAD_T))
                .pb(px(HALF_ROW))
                .text_size(px(FS_PROSE))
                .line_height(px(LH_PROSE))
                .child(ferrite_pane::rich::Markdown::new(
                    format!("files-{}", document.path.display()),
                    document.rich_source().unwrap_or_default(),
                    rich.clone(),
                ))
                .into_any_element(),
            TabBody::Unshown(why) => div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_size(px(FS_LABEL))
                .text_color(rgb(INK_SUBTLE))
                .child(*why)
                .into_any_element(),
        };
        column.child(breadcrumb).child(body)
    }

    fn files_tree(&self, thread: ThreadId, files: &Files, cx: &mut Context<Self>) -> Div {
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
            .on_action(cx.listener(move |view, _: &OpenFirst, _, cx| view.open_first_match(thread, cx)))
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
                let rows = files.tree.rows();
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
                rows.into_iter()
                    .take(FILTER_ROWS.max(1_000))
                    .enumerate()
                    .map(|(at, row)| self.tree_row(thread, at, row, active.as_deref(), cx)),
            );
        div()
            .debug_selector(|| "files-tree-column".into())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(FILES_TREE_W))
            .h_full()
            .gap(px(SPACE_1))
            .child(div().h(px(ICON_BUTTON - SPACE_2)).flex_shrink_0())
            .child(field)
            .child(body)
    }

    fn tree_row(
        &self,
        thread: ThreadId,
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
                    cx.notify();
                } else {
                    // A second press keeps the tab (`ClickEvent::click_count`).
                    let keep = event.click_count() >= 2;
                    view.open_in_files(thread, press.clone(), None, keep, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if let Some(files) = view.files_of(thread) {
                        files.pressed = Some(menu_path.clone());
                    }
                    view.open_context_menu(MenuTarget::File(thread), event.position, cx);
                }),
            )
    }
}

#[cfg(test)]
impl CockpitView {
    /// Each open tab's name and whether it is a preview tab.
    pub(in crate::cockpit) fn files_tab_names(&self, thread: ThreadId) -> Vec<(String, bool)> {
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

    /// The path of the shown tab.
    pub(in crate::cockpit) fn files_active_path(&self, thread: ThreadId) -> Option<PathBuf> {
        self.files_ref(thread)?.active_path().map(Path::to_path_buf)
    }

    /// The shown tab's editor, when it is a code file.
    pub(in crate::cockpit) fn files_editor_of(&self, thread: ThreadId) -> Option<Entity<Editor>> {
        let files = self.files_ref(thread)?;
        match &files.tabs[files.active?].body {
            TabBody::Code(editor) => Some(editor.clone()),
            _ => None,
        }
    }

    pub(in crate::cockpit) fn set_files_filter_for_test(
        &mut self,
        thread: ThreadId,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(files) = self.files_ref(thread) {
            let field = files.filter.clone();
            field.update(cx, |field, cx| field.set(text.to_string(), cx));
        }
    }
}
