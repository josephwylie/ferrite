//! A finished turn's cards: what it left to open
//! (`ferrite_core::transcript::outputs`), drawn under its answer
//! (`RowKind::Outputs`, theme `OUTPUT_*`, `CHANGES_*`).
//!
//! - The **changed-files card** (T3's): how many files the turn changed
//!   and the lines gained and lost, `Expand all` and `Open diff`, over the
//!   files as a tree of shut folders. `Open diff`, or a press on a file,
//!   opens the Changes Surface (`TranscriptEvent::OpenChanges`).
//! - A **page card** (Claude's; an `.html` file, a local address): its
//!   thumbnail, its title and what it is, `Open` — the page beside the
//!   transcript, in the Panel (`Preview::open_page`) — and `⋯`: open in
//!   the browser, copy its path or address. Each, and each of its doors,
//!   is told apart by its place among the turn's (`page-card-open-1`).
//!
//! A path resolves against the Thread's checkout, as every path target
//! does (`FileLink::resolve`).

use std::path::Path;

use ferrite_core::transcript::outputs::{Changed, Output, Page};
use ferrite_core::transcript::BlockId;
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, ClipboardItem, Context, MouseButton, Pixels, Point};

use super::{
    rows::{RowId, TranscriptRow},
    TranscriptEvent, TranscriptView,
};
use crate::pane::DisclosureId;
use crate::file_links::FileLink;
use crate::page::PageSource;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::menu;
use ferrite_ui::pointer::Pointer;
use ferrite_ui::theme::{self, paint};

/// A card's menu while it is open: what it is for and where it hangs.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct OutputMenu {
    target: Opened,
    at: Point<Pixels>,
}

/// What a card opens, its path resolved.
#[derive(Clone, Debug, PartialEq)]
enum Opened {
    Page(PageSource),
}

/// One row of a card's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verb {
    Browser,
    Copy,
}

impl Opened {
    fn verbs(&self) -> &'static [(&'static str, Verb)] {
        match self {
            Opened::Page(PageSource::File(_)) => &[
                ("Open in browser", Verb::Browser),
                ("Copy path", Verb::Copy),
            ],
            Opened::Page(PageSource::Address(_)) => &[
                ("Open in browser", Verb::Browser),
                ("Copy address", Verb::Copy),
            ],
        }
    }

    fn run(&self, verb: Verb, cx: &mut gpui::App) {
        match (self, verb) {
            (Opened::Page(page), Verb::Browser) => cx.open_url(&page.url()),
            (Opened::Page(PageSource::File(path)), Verb::Copy) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()))
            }
            (Opened::Page(PageSource::Address(address)), Verb::Copy) => {
                cx.write_to_clipboard(ClipboardItem::new_string(address.clone()))
            }
        }
    }
}

impl TranscriptView {
    /// Resolve an output against the Thread's checkout.
    fn opened(&self, output: &Output) -> Option<Opened> {
        let resolve = |path: &str| {
            FileLink::resolve(path, self.input.workspace.as_deref()).map(|link| link.path)
        };
        Some(match output {
            Output::Page(Page::File(path)) => Opened::Page(PageSource::File(resolve(path)?)),
            Output::Page(Page::Address(address)) => {
                Opened::Page(PageSource::Address(address.clone()))
            }
            Output::Changes(_) => return None,
        })
    }

    /// An outputs row: its changed-files card, then its page cards, one
    /// under another.
    pub(super) fn render_outputs(&self, row: &TranscriptRow, cx: &mut Context<Self>) -> AnyElement {
        let key = format!("{}-{:?}", self.input.namespace, row.id());
        let end = match row.id() {
            RowId::Outputs(end) => Some(*end),
            _ => None,
        };
        let changes = row.outputs().iter().find_map(|output| match output {
            Output::Changes(changes) => Some(changes.as_slice()),
            _ => None,
        });
        div()
            .debug_selector(|| "transcript-outputs".into())
            .flex()
            .flex_col()
            .gap(px(theme::OUTPUT_CARD_GAP))
            .children(
                changes
                    .zip(end)
                    .map(|(changes, end)| self.changes_card(&key, end, changes, cx)),
            )
            .children(
                row.outputs()
                    .iter()
                    .filter_map(|output| self.opened(output))
                    .enumerate()
                    .map(|(index, opened)| card(&key, index, opened, cx)),
            )
            .into_any_element()
    }

    /// The changed-files card: its head, then the tree as far as its
    /// folders are open.
    fn changes_card(
        &self,
        key: &str,
        end: BlockId,
        changes: &[Changed],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let all = self.input.expanded.contains(&DisclosureId::Changes(end));
        let nodes = tree(changes, self.input.workspace.as_deref());
        let mut rows = Vec::new();
        self.tree_rows(key, end, all, &nodes, 0, &mut rows, cx);
        let added: usize = changes.iter().map(|change| change.added).sum();
        let removed: usize = changes.iter().map(|change| change.removed).sum();
        let files = changes.len();
        let toggle = DisclosureId::Changes(end);
        let head = div()
            .debug_selector(|| "changes-card-head".into())
            .flex()
            .items_center()
            .gap(px(theme::SPACE_2))
            .h(px(theme::CHANGES_HEAD_H))
            .px(px(theme::CHANGES_PAD_X))
            .border_b_1()
            .border_color(paint::LINE)
            .child(
                div()
                    .debug_selector(|| "changes-card-title".into())
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(theme::SPACE_2))
                    .text_size(px(theme::FS_META))
                    .line_height(px(theme::LH_META))
                    .child(
                        div()
                            .font_weight(theme::W_LABEL)
                            .text_color(rgb(theme::INK_STRONG))
                            .child(changed_label(files)),
                    )
                    .child(counts(added, removed)),
            )
            .child(
                text_door(
                    &format!("{key}-changes-all"),
                    "changes-card-expand".into(),
                    if all { "Collapse all" } else { "Expand all" },
                    false,
                )
                .on_click(cx.listener(move |_, _: &gpui::ClickEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(TranscriptEvent::ToggleDisclosure(toggle.clone()));
                })),
            )
            .child(
                text_door(
                    &format!("{key}-changes-open"),
                    "changes-card-open".into(),
                    "Open diff",
                    false,
                )
                .on_click(cx.listener(|_, _: &gpui::ClickEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(TranscriptEvent::OpenChanges(None));
                })),
            );
        div()
            .debug_selector(|| "changes-card".into())
            .flex()
            .flex_col()
            .w_full()
            .rounded(px(theme::R_CARD))
            .border_1()
            .border_color(paint::LINE)
            .overflow_hidden()
            .child(head)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .py(px(theme::CHANGES_TREE_PAD_Y))
                    .children(rows),
            )
            .into_any_element()
    }

    /// The tree's rows from `nodes` down, `depth` levels in, as far as
    /// its folders are open.
    #[allow(clippy::too_many_arguments)]
    fn tree_rows(
        &self,
        key: &str,
        end: BlockId,
        all: bool,
        nodes: &[Node<'_>],
        depth: usize,
        rows: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        for node in nodes {
            match node {
                Node::Folder {
                    name,
                    key: folder,
                    children,
                } => {
                    let id = DisclosureId::Folder(end, folder.clone());
                    let open = all != self.input.expanded.contains(&id);
                    let selector = format!("changes-folder-{folder}");
                    rows.push(
                        tree_row(&format!("{key}-dir-{folder}"), selector, depth)
                            .child(icon(
                                if open { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT },
                                theme::CHANGES_GLYPH,
                                theme::TEXT_FAINT,
                            ))
                            .child(icon(icons::FOLDER, theme::CHANGES_GLYPH, theme::INK_SUBTLE))
                            .child(name_cell(name.clone()))
                            .on_click(cx.listener(move |_, _: &gpui::ClickEvent, _, cx| {
                                cx.emit(TranscriptEvent::ToggleDisclosure(id.clone()));
                            }))
                            .into_any_element(),
                    );
                    if open {
                        self.tree_rows(key, end, all, children, depth + 1, rows, cx);
                    }
                }
                Node::File { name, change } => {
                    let path = change.path.clone();
                    let selector = format!("changes-file-{}", change.path);
                    rows.push(
                        tree_row(&format!("{key}-file-{}", change.path), selector, depth)
                            .child(div().size(px(theme::CHANGES_GLYPH)).flex_shrink_0())
                            .child(icon(icons::FILE, theme::CHANGES_GLYPH, theme::INK_SUBTLE))
                            .child(name_cell(name.clone()))
                            .child(counts(change.added, change.removed))
                            .on_click(cx.listener(move |_, _: &gpui::ClickEvent, _, cx| {
                                cx.emit(TranscriptEvent::OpenChanges(Some(path.clone())));
                            }))
                            .into_any_element(),
                    );
                }
            }
        }
    }

    fn open_output_menu(&mut self, target: Opened, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.output_menu = Some(OutputMenu { target, at });
        cx.notify();
    }

    /// The open card's menu, hung at the press that opened it.
    pub(super) fn output_menu_float(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.output_menu.clone()?;
        let mut shell = menu::shell()
            .debug_selector(|| "output-menu".into())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|view, _: &gpui::MouseDownEvent, _, cx| {
                view.output_menu = None;
                cx.notify();
            }));
        for (index, (label, verb)) in open.target.verbs().iter().copied().enumerate() {
            let target = open.target.clone();
            shell = shell.child(
                menu::row(index, &menu::Item::new(label), false)
                    .debug_selector(move || format!("output-menu-row-{index}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &gpui::MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.output_menu = None;
                            target.run(verb, cx);
                            cx.notify();
                        }),
                    ),
            );
        }
        Some(super::hang_menu("output-menu-in", open.at, shell))
    }
}

/// One card. A press on its body opens it; its doors keep their presses.
fn card(row: &str, index: usize, opened: Opened, cx: &mut Context<TranscriptView>) -> AnyElement {
    let key = format!("{row}-{index}");
    let key = key.as_str();
    let (selector, lead, title, detail) = match &opened {
        Opened::Page(page) => (
            "page-card",
            thumbnail(),
            page.title(),
            page.kind().to_owned(),
        ),
    };
    let doors: Vec<AnyElement> = match &opened {
        Opened::Page(page) => {
            let shown = page.clone();
            vec![
                text_door(
                    &format!("{key}-open"),
                    format!("page-card-open-{index}"),
                    "Open",
                    false,
                )
                .on_click(cx.listener(move |view, _: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    view.input.preview.open_page(shown.clone(), window);
                }))
                .into_any_element(),
                more_door(&format!("{key}-more"), index, opened.clone(), cx),
            ]
        }
    };
    let body = opened.clone();
    let hover = ferrite_ui::pointer::hover_key(&gpui::ElementId::from(gpui::SharedString::from(
        key.to_owned(),
    )));
    div()
        .id(gpui::SharedString::from(key.to_owned()))
        .debug_selector(move || format!("{selector}-{index}"))
        .flex()
        .items_center()
        .gap(px(theme::SPACE_3))
        .w_full()
        .max_w(px(theme::OUTPUT_CARD_W))
        .h(px(theme::OUTPUT_CARD_H))
        .p(px(theme::OUTPUT_CARD_PAD))
        .rounded(px(theme::R_CARD))
        .bg(paint::RAISED)
        .border_1()
        .border_color(paint::LINE)
        .cursor_pointer()
        .hover_raised(hover)
        .child(lead)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(px(theme::FS_UI))
                        .line_height(px(theme::LH_UI))
                        .font_weight(theme::W_LABEL)
                        .text_color(rgb(theme::INK_STRONG))
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(theme::FS_META))
                        .line_height(px(theme::LH_META))
                        .text_color(rgb(theme::INK_SUBTLE))
                        .child(detail),
                ),
        )
        .children(doors)
        .on_click(
            cx.listener(move |view, _: &gpui::ClickEvent, window, _| match &body {
                Opened::Page(page) => view.input.preview.open_page(page.clone(), window),
            }),
        )
        .into_any_element()
}

/// A page's thumbnail: the globe on the card's own ground until a picture
/// of it is drawn.
fn thumbnail() -> AnyElement {
    let inner = theme::OUTPUT_CARD_H - 2. * theme::OUTPUT_CARD_PAD - 2.;
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(theme::OUTPUT_THUMB_W))
        .h(px(inner))
        .rounded(px(theme::R_CONTROL))
        .bg(paint::CARD)
        .child(icon(
            icons::GLOBE,
            theme::ICON_BUTTON_GLYPH,
            theme::INK_SUBTLE,
        ))
        .into_any_element()
}

/// `N changed file(s)`.
fn changed_label(files: usize) -> String {
    format!("{files} changed file{}", if files == 1 { "" } else { "s" })
}

/// Lines gained and lost, `+12 −3`, in the diff's signs.
fn counts(added: usize, removed: usize) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .gap(px(theme::SPACE_1))
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .child(div().text_color(rgb(theme::DIFF_ADDED_SIGN)).child(format!("+{added}")))
        .child(div().text_color(rgb(theme::DIFF_REMOVED_SIGN)).child(format!("−{removed}")))
}

/// One row of the tree, `depth` levels in, lit under the pointer.
fn tree_row(id: &str, selector: String, depth: usize) -> gpui::Stateful<gpui::Div> {
    let id = gpui::ElementId::from(gpui::SharedString::from(id.to_owned()));
    let hover = ferrite_ui::pointer::hover_key(&id);
    div()
        .id(id)
        .debug_selector(move || selector)
        .flex()
        .items_center()
        .gap(px(theme::SPACE_1))
        .h(px(theme::CHANGES_ROW_H))
        .pl(px(theme::CHANGES_INDENT + theme::CHANGES_INDENT_STEP * depth as f32))
        .pr(px(theme::CHANGES_PAD_X))
        .cursor_pointer()
        .hover_raised(hover)
}

fn name_cell(name: String) -> gpui::Div {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::INK_STRONG))
        .child(name)
}

/// One node of a changed-files tree: a folder, a chain of folders that
/// each hold only the next read as one (T3's `src/ui`), or a file.
#[derive(Debug, PartialEq)]
pub(crate) enum Node<'a> {
    Folder {
        name: String,
        /// The folder's whole path under the root: its disclosure's key.
        key: String,
        children: Vec<Node<'a>>,
    },
    File {
        name: String,
        change: &'a Changed,
    },
}

/// The files `changes` names as a tree under `root` (a path outside it
/// as it is), folders before files, each by name.
pub(crate) fn tree<'a>(changes: &'a [Changed], root: Option<&Path>) -> Vec<Node<'a>> {
    #[derive(Default)]
    struct Dir<'a> {
        dirs: std::collections::BTreeMap<String, Dir<'a>>,
        files: Vec<(String, &'a Changed)>,
    }
    fn nodes<'a>(dir: Dir<'a>, under: &str) -> Vec<Node<'a>> {
        let mut out = Vec::new();
        for (mut name, mut child) in dir.dirs {
            while child.files.is_empty() && child.dirs.len() == 1 {
                let (next, grand) = child.dirs.into_iter().next().unwrap();
                name = format!("{name}/{next}");
                child = grand;
            }
            let key = if under.is_empty() {
                name.clone()
            } else {
                format!("{under}/{name}")
            };
            let children = nodes(child, &key);
            out.push(Node::Folder {
                name,
                key,
                children,
            });
        }
        let mut files = dir.files;
        files.sort_by(|a, b| a.0.cmp(&b.0));
        out.extend(files.into_iter().map(|(name, change)| Node::File { name, change }));
        out
    }
    let mut top = Dir::default();
    for change in changes {
        let shown = root
            .and_then(|root| crate::file_links::inside_checkout(Path::new(&change.path), root))
            .map_or_else(|| change.path.clone(), |path| path.display().to_string());
        let mut parts: Vec<&str> = shown.split('/').filter(|part| !part.is_empty()).collect();
        let Some(name) = parts.pop() else {
            continue;
        };
        let mut dir = &mut top;
        for part in parts {
            dir = dir.dirs.entry(part.to_owned()).or_default();
        }
        dir.files.push((name.to_owned(), change));
    }
    nodes(top, "")
}

/// Every folder's key in `nodes`, all the way down.
pub(crate) fn folders(nodes: &[Node<'_>]) -> Vec<String> {
    let mut keys = Vec::new();
    for node in nodes {
        if let Node::Folder { key, children, .. } = node {
            keys.push(key.clone());
            keys.extend(folders(children));
        }
    }
    keys
}

/// A word door at a card's right (`Open`, `Expand all`), lit under the
/// pointer; a press on it never reaches the card.
fn text_door(
    id: &str,
    selector: String,
    label: &'static str,
    chevron: bool,
) -> gpui::Stateful<gpui::Div> {
    let id = gpui::ElementId::from(gpui::SharedString::from(id.to_owned()));
    let hover = ferrite_ui::pointer::hover_key(&id);
    div()
        .id(id)
        .debug_selector(move || selector)
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_1))
        .h(px(theme::TOKEN_H + theme::SPACE_1 * 2.))
        .px(px(theme::SPACE_2))
        .rounded(px(theme::R_CONTROL))
        .border_1()
        .border_color(paint::LINE)
        .text_size(px(theme::FS_META))
        .text_color(rgb(theme::INK_STRONG))
        .hover_raised(hover)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(label)
        .when(chevron, |door| {
            door.child(icon(
                icons::CHEVRON_RIGHT,
                theme::FS_META,
                theme::INK_SUBTLE,
            ))
        })
}

/// A page card's `⋯`.
fn more_door(
    id: &str,
    index: usize,
    opened: Opened,
    cx: &mut Context<TranscriptView>,
) -> AnyElement {
    let id = gpui::ElementId::from(gpui::SharedString::from(id.to_owned()));
    let hover = ferrite_ui::pointer::hover_key(&id);
    div()
        .id(id)
        .debug_selector(move || format!("page-card-more-{index}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(theme::ICON_BUTTON))
        .rounded(px(theme::R_CONTROL))
        .hover_raised(hover)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(icon(
            icons::ELLIPSIS,
            theme::ICON_BUTTON_GLYPH,
            theme::INK_SUBTLE,
        ))
        .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
            cx.stop_propagation();
            view.open_output_menu(opened.clone(), event.position(), cx);
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_card_offers_its_own_verbs() {
        let labels = |opened: Opened| -> Vec<&'static str> {
            opened.verbs().iter().map(|(label, _)| *label).collect()
        };
        assert_eq!(
            labels(Opened::Page(PageSource::Address(
                "http://localhost:3000".into()
            ))),
            ["Open in browser", "Copy address"]
        );
        assert_eq!(
            labels(Opened::Page(PageSource::File(std::path::PathBuf::from("/w/a.html")))),
            ["Open in browser", "Copy path"]
        );
    }

    /// The files read as T3's tree: under the checkout, folders first,
    /// a chain of lone folders as one, each level its own.
    #[test]
    fn changed_files_read_as_a_tree() {
        let change = |path: &str| Changed {
            path: path.into(),
            added: 1,
            removed: 0,
        };
        let changes = [
            change("/w/src/ui/button.rs"),
            change("/w/README.md"),
            change("/w/src/ui/card.rs"),
            change("/w/src/main.rs"),
        ];
        let names = |nodes: &[Node<'_>]| -> Vec<String> {
            nodes
                .iter()
                .map(|node| match node {
                    Node::Folder { name, .. } => format!("{name}/"),
                    Node::File { name, .. } => name.clone(),
                })
                .collect()
        };
        let top = tree(&changes, Some(Path::new("/w")));
        assert_eq!(names(&top), ["src/", "README.md"]);
        let Node::Folder { children, key, .. } = &top[0] else {
            panic!("src is a folder");
        };
        assert_eq!(key, "src");
        assert_eq!(names(children), ["ui/", "main.rs"]);
        assert_eq!(folders(&top), ["src", "src/ui"]);
        let lone = [change("/w/a/b/c.rs")];
        let top = tree(&lone, Some(Path::new("/w")));
        assert_eq!(names(&top), ["a/b/"]);
        // The agent names the file by its real path; the Thread stands in
        // the checkout through a symlink: the file is still under it.
        let (real, link) = crate::file_links::tests::symlinked_checkout("changed-tree");
        let linked = [change(&real.join("f01.txt").to_string_lossy())];
        assert_eq!(names(&tree(&linked, Some(&link))), ["f01.txt"]);
        assert_eq!(changed_label(1), "1 changed file");
        assert_eq!(changed_label(3), "3 changed files");
    }
}
