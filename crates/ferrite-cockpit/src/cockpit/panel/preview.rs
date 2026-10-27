//! The Panel's Preview tab (SPEC §5.6.3): a Pane's preview
//! (`crate::preview`) read in the right Panel — a file at a line (numbered
//! lines, the target marked and centred, in syntax colour), rendered
//! Markdown, a command's output, an image in the image viewer (fit and
//! actual size, zoom, pinch, pan, ← → through the images it was opened
//! among), or a note for a file it cannot show, its original a click away.
//!
//! Whatever opens a file in a Pane goes through `Preview::open` — the
//! cockpit's own entry points (a file link, a ⌘-clicked path, a changed
//! file, a review's line, the empty board) through `open_preview` — and the
//! cockpit sees it on its next frame (`tend_previews`): the Panel's head
//! grows a Preview tab, the file's name cut from its start, and selects it;
//! a line opened at is scrolled to the middle of the list. A Panel that was
//! hidden shows for it, and hides again when it closes. Another file opening
//! over it replaces it in place, its body fading in (`motion::swap_in`); an
//! image stepping to the next does not, the viewer crossfading the two
//! itself. Escape in it (the `Reader` key context, deeper than the Cockpit's
//! interrupt), its × or ⌘W while it holds the keyboard closes it, and the
//! Panel goes back to the tab it showed before. ⌘F finds in it
//! (`PreviewFind`, the transcript's bar).
//!
//! While the Preview tab is the one shown the Panel is wide (`preview_w`,
//! the theme's `PANEL_PREVIEW_*`), riding there and back with the board
//! (`ride_panel`).
//!
//! Each Pane keeps its own preview (`PaneView::preview_tab`), and the Panel
//! shows the focused Pane's: focus moving to another Thread leaves the
//! preview where it is, and coming back finds it as it was. A draft, which
//! has no Panel of its own, shows the Panel for its preview alone (the
//! Preview tab, no other), and its first send keeps it as it was.

use super::*;
use crate::preview::{head_door, head_suffix, head_title, reading_head, Body, Text, Unshown};
use std::path::{Path, PathBuf};
use std::sync::Arc;

gpui::actions!(reader, [Close]);

/// What the Panel keeps of one Pane's preview, on the Pane.
pub(crate) struct PreviewTab {
    /// The open last tended (`Preview::opened`).
    seen: Option<u64>,
    /// The Preview tab is the one the Panel shows for its Pane.
    selected: bool,
    /// The document shown opened after the first frame: its body fades in
    /// under this key (`Preview::opened`'s), so an image stepping to the
    /// next, one document to the viewer, does not fade again.
    fade: Option<u64>,
    /// The numbered lines' own scroll, kept so a line opened at can be
    /// scrolled into view.
    pub(crate) scroll: gpui::UniformListScrollHandle,
    /// A rendered Markdown document's own scroll.
    pub(crate) markdown_scroll: ScrollHandle,
    /// The Markdown document's native text.
    rich: crate::rich::TextCache,
    /// The find bar, while it is open.
    pub(crate) find: Option<PreviewFind>,
}

impl Default for PreviewTab {
    fn default() -> Self {
        Self {
            seen: None,
            selected: false,
            fade: None,
            scroll: gpui::UniformListScrollHandle::new(),
            markdown_scroll: ScrollHandle::new(),
            rich: crate::rich::TextCache::default(),
            find: None,
        }
    }
}

/// Find in the preview (⌘F when the preview took the last press or holds
/// the keyboard; the bar is the transcript's, `crate::find`): a file's
/// lines, or a rendered Markdown document's text.
pub(crate) struct PreviewFind {
    bar: crate::find::Find,
    /// The document it searches: the bar closes with it, and another one
    /// opening closes it too (`tend_previews`).
    path: PathBuf,
    /// Frames left to settle a Markdown occurrence into view.
    reveal: u8,
    /// A new query in a Markdown document, waiting for its paint to say
    /// which occurrence is the first in view.
    start_in_view: bool,
}

impl PreviewFind {
    /// Whether the bar's line holds the keyboard.
    pub(crate) fn holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.bar.holds_focus(window, cx)
    }
}

#[cfg(test)]
impl PreviewFind {
    /// What the bar reads, and where a Markdown occurrence last painted.
    pub(crate) fn state(&self) -> (Option<SharedString>, Option<gpui::Bounds<Pixels>>) {
        (self.bar.count(), self.bar.report().revealed.get())
    }
}

impl CockpitView {
    /// Open `path` in the focused Pane's preview, at `line` (see the module
    /// doc): the cockpit's entry points route here, as the Pane's own go to
    /// `Preview::open`.
    pub(crate) fn open_preview(
        &mut self,
        path: PathBuf,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.empty_shown = false;
        if let Some(pane) = self.panes.get(self.focused()) {
            pane.preview.open(path, line, &[], window, cx);
        }
        cx.notify();
    }

    /// Whether Pane `index`'s Panel shows its Preview tab: a preview is
    /// open and selected — always, for a draft, which has no other tab.
    pub(in crate::cockpit) fn preview_shown(&self, index: usize) -> bool {
        self.panes.get(index).is_some_and(|pane| {
            pane.preview.is_open() && (pane.preview_tab.selected || pane.thread().is_none())
        })
    }

    /// The Preview tab selected for `owner`'s Pane.
    fn select_preview_tab(&mut self, owner: PaneIdentity, cx: &mut Context<Self>) {
        if let Some(index) = self.index_of(owner) {
            self.panes[index].preview_tab.selected = true;
        }
        cx.notify();
    }

    /// Another tab selected for `thread`'s Pane: its preview stays open,
    /// its tab still in the head, and coming back to it does not fade.
    pub(in crate::cockpit) fn deselect_preview_tab(&mut self, thread: ThreadId) {
        if let Some(index) = self.pane_for(thread) {
            let tab = &mut self.panes[index].preview_tab;
            tab.selected = false;
            tab.fade = None;
        }
    }

    /// Close Pane `index`'s preview (escape in it, its ×, ⌘W while it holds
    /// focus): the keyboard is the Pane's, and the Panel goes back to the
    /// tab it showed before — or, shown for the preview alone, hides.
    pub(in crate::cockpit) fn close_preview(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(index) {
            pane.preview.close();
            self.focus_pane(index);
            cx.notify();
        }
    }

    /// Each frame, before the Panel rides: a preview that opened selects
    /// the Preview tab (marked to fade in, after the first frame) and
    /// scrolls its line to the middle of the list; one that closed hands
    /// the Panel back to the tab it showed before. A find bar closes with
    /// its document, and a Markdown reveal that is due settles.
    pub(in crate::cockpit) fn tend_previews(&mut self, window: &mut Window) {
        let first = !std::mem::replace(&mut self.panel.tended, true);
        for pane in &mut self.panes {
            let opened = pane.preview.opened();
            let document = pane.preview.document();
            let tab = &mut pane.preview_tab;
            match opened {
                Some((open, key)) if tab.seen != Some(open) => {
                    tab.selected = true;
                    tab.fade = (!first).then_some(key);
                    if let Some(line) = document
                        .as_ref()
                        .and_then(|document| document.line())
                        .filter(|line| *line > 0)
                    {
                        tab.scroll
                            .scroll_to_item(line as usize - 1, gpui::ScrollStrategy::Center);
                    }
                }
                None if tab.seen.is_some() => {
                    tab.selected = false;
                    tab.fade = None;
                }
                _ => {}
            }
            tab.seen = opened.map(|(open, _)| open);
            let Some(find) = &mut tab.find else {
                continue;
            };
            if document.is_none_or(|document| document.path != find.path) {
                tab.find = None;
                continue;
            }
            settle_markdown_found(find, &tab.markdown_scroll, window);
        }
    }

    /// The Preview tab's label for `owner`'s Pane while it has one to show:
    /// the file's name, or a command's.
    pub(in crate::cockpit) fn preview_tab_label(
        &self,
        owner: PaneIdentity,
    ) -> Option<SharedString> {
        let document = self.panes[self.index_of(owner)?].preview.shown()?;
        let name = document.name.clone().unwrap_or_else(|| {
            document
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        Some(name.into())
    }

    /// The Preview tab in the head: the Panel's tab grammar, its label the
    /// file's name cut from the start (`PANEL_PREVIEW_TAB_MAX_W`).
    pub(in crate::cockpit) fn preview_tab_element(
        &self,
        owner: PaneIdentity,
        label: SharedString,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::theme::*;
        let id = gpui::ElementId::from("panel-tab-Preview");
        let key = crate::pointer::hover_key(&id);
        div()
            .id(id)
            .debug_selector(|| "panel-tab-Preview".into())
            .flex()
            .flex_shrink_1()
            .min_w_0()
            .max_w(px(PANEL_PREVIEW_TAB_MAX_W))
            .items_center()
            .h(px(PANEL_TAB_H))
            .px(px(PANEL_TAB_PAD_X))
            .rounded(px(R_CONTROL))
            .text_size(px(FS_LABEL))
            .font_weight(W_LABEL)
            .occlude()
            .map(|face| {
                if active {
                    face.hover_carried(key).text_color(rgb(TEXT_STRONG))
                } else {
                    face.text_color(rgb(TEXT_MUTED))
                        .hover_raised(key)
                        .press_raised()
                }
            })
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis_start()
                    .child(label),
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.select_preview_tab(owner, cx);
            }))
            .into_any_element()
    }

    /// The Preview tab's body: the preview as a card on the Panel's frame,
    /// the board's cards' twin (`R_PANE` on `CARD`, ringed), its top and
    /// foot level with theirs and its head level with the Pane head beside
    /// it. The head (`reading_head`) reads the file's path from the
    /// Thread's workspace in `LINK_INK` and its `:line` dim — an image its
    /// name and its place among its gallery's, `· 2/3` — then the original
    /// (an image, a file it cannot show) and the ×. A press lands on the
    /// Pane that opened it; escape closes it.
    pub(in crate::cockpit) fn panel_preview(
        &self,
        owner: PaneIdentity,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::theme::*;
        let Some(index) = self.index_of(owner) else {
            return div().into_any_element();
        };
        let pane = &self.panes[index];
        let Some(document) = pane.preview.shown() else {
            return div().into_any_element();
        };
        let tab = &pane.preview_tab;
        let find = tab.find.as_ref();
        let body = match &document.body {
            Body::Image { viewer, .. } => viewer.clone().into_any_element(),
            Body::Text { text, .. } if document.renders_markdown() => {
                tab.rich.file_context(document.path.parent(), &pane.preview);
                let marks = find.and_then(|find| find.bar.marks(0));
                div()
                    .id("preview-markdown")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&tab.markdown_scroll)
                    .px(px(TX_INSET_SOLO))
                    .pt(px(BODY_PAD_T))
                    .pb(px(HALF_ROW))
                    .child(
                        crate::rich::Markdown::new(
                            format!("document-{}", document.path.display()),
                            text.source.clone(),
                            tab.rich.clone(),
                        )
                        .find(marks),
                    )
                    .into_any_element()
            }
            Body::Text { text, line } => numbered_lines(
                &document.path,
                text.clone(),
                *line,
                tab.scroll.clone(),
                find.map(|find| &find.bar),
            ),
            Body::Unshown(why) => unshown(why, &document.path, cx),
        };
        // The bar hangs over the body's top right.
        let body = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(body)
            .children(find.map(|find| find.bar.bar()));
        let body = match tab.fade {
            Some(key) => {
                crate::motion::swap_in(SharedString::from(format!("preview-swap-{key}")), body)
                    .into_any_element()
            }
            None => body.into_any_element(),
        };
        let title = self.preview_title(pane, &document);
        let label = match &document.body {
            Body::Image { gallery, at, .. } => {
                head_title(title).children((gallery.len() > 1).then(|| {
                    head_suffix(format!(" \u{b7} {}/{}", at + 1, gallery.len()))
                        .id("reader-image-count")
                        .tooltip(|window, cx| {
                            gpui::component::tooltip::Tooltip::new(
                                "Step through them with \u{2190} \u{2192}",
                            )
                            .build(window, cx)
                        })
                }))
            }
            _ => head_title(title)
                .children(document.line().map(|line| head_suffix(format!(":{line}")))),
        };
        let original = matches!(document.body, Body::Image { .. } | Body::Unshown(_))
            .then(|| document.path.clone());
        let doors = original
            .map(|original| {
                head_door(
                    "open-original-image",
                    "preview-original".into(),
                    crate::icons::EXTERNAL,
                )
                .tooltip(|window, cx| {
                    gpui::component::tooltip::Tooltip::new("Open the original in the default app")
                        .build(window, cx)
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    crate::preview::open_original(&original, window, cx);
                })
                .into_any_element()
            })
            .into_iter()
            .chain([
                head_door("preview-close", "preview-close".into(), crate::icons::CLOSE)
                    .tooltip(crate::menu::tooltip_with_key(
                        "Close the preview",
                        crate::components::bound_chord_in(
                            "reader::Close",
                            Some(crate::keymap::READER),
                        ),
                    ))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        if let Some(index) = view.index_of(owner) {
                            view.close_preview(index, cx);
                        }
                    }))
                    .into_any_element(),
            ]);
        div()
            .debug_selector(|| "panel-preview".into())
            .track_focus(pane.preview.focus_handle())
            .key_context(crate::keymap::READER)
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .rounded(px(R_PANE))
            .bg(paint::CARD)
            .whitespace_normal()
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .child(reading_head(label, doors))
            .child(body)
            .child(pane::card_ring(rgba(CARD_RING)))
            .on_action(cx.listener(move |view, _: &Close, _, cx| {
                if let Some(index) = view.index_of(owner) {
                    view.close_preview(index, cx);
                }
            }))
            .on_action(cx.listener(move |view, _: &crate::find::Next, _, cx| {
                view.step_preview_find(owner, true, cx)
            }))
            .on_action(cx.listener(move |view, _: &crate::find::Previous, _, cx| {
                view.step_preview_find(owner, false, cx)
            }))
            .on_action(
                cx.listener(move |view, _: &crate::find::Close, window, cx| {
                    view.close_preview_find(owner, window, cx)
                }),
            )
            .on_action(
                cx.listener(move |view, _: &crate::image_viewer::Next, _, cx| {
                    view.step_image(owner, 1, cx)
                }),
            )
            .on_action(
                cx.listener(move |view, _: &crate::image_viewer::Previous, _, cx| {
                    view.step_image(owner, -1, cx)
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    view.preview_pressed = Some(owner);
                    if let Some(index) = view.index_of(owner) {
                        view.focus_pane(index);
                        cx.notify();
                    }
                }),
            )
            .into_any_element()
    }

    /// What the head reads for `document`: a command's name, an image's,
    /// else the path from the Thread's workspace, else from `~`.
    fn preview_title(&self, pane: &PaneView, document: &crate::preview::Document) -> String {
        if let Some(name) = &document.name {
            return name.clone();
        }
        if matches!(document.body, Body::Image { .. }) {
            return document
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        }
        pane.thread()
            .and_then(|thread| self.thread_path(thread))
            .and_then(|root| {
                document
                    .path
                    .strip_prefix(root)
                    .ok()
                    .map(|relative| relative.display().to_string())
            })
            .unwrap_or_else(|| home_relative(&document.path))
    }

    /// ← → in an image in `owner`'s preview: the next or the previous of
    /// the images it was opened among.
    fn step_image(&mut self, owner: PaneIdentity, by: isize, cx: &mut Context<Self>) {
        if let Some(index) = self.index_of(owner) {
            if self.panes[index].preview.step_image(by, cx) {
                cx.notify();
            }
        }
    }

    /// ⌘F in Pane `index`'s preview: the bar over it (or the keyboard back
    /// to it). False when the preview shows nothing to search.
    pub(in crate::cockpit) fn open_preview_find(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self.panes.get_mut(index) else {
            return false;
        };
        let Some(document) = pane
            .preview
            .document()
            .filter(|document| document.text().is_some())
        else {
            return false;
        };
        let owner = pane.identity;
        let tab = &mut pane.preview_tab;
        if tab
            .find
            .as_ref()
            .is_none_or(|find| find.path != document.path)
        {
            tab.find = Some(PreviewFind {
                bar: crate::find::Find::new(cx, move |view: &mut Self, cx| {
                    view.refind_preview(owner, cx)
                }),
                path: document.path,
                reveal: 0,
                start_in_view: false,
            });
            cx.notify();
        }
        if let Some(find) = &tab.find {
            find.bar.focus(window, cx);
        }
        true
    }

    /// Search the preview's document again (its line changed): a new query
    /// starts on its first occurrence in view, or the first below it.
    fn refind_preview(&mut self, owner: PaneIdentity, cx: &mut Context<Self>) {
        let Some(pane) = self.index_of(owner).map(|index| &mut self.panes[index]) else {
            return;
        };
        let Some(document) = pane.preview.document() else {
            return;
        };
        let Some(text) = document.text() else {
            return;
        };
        let tab = &mut pane.preview_tab;
        let Some(find) = tab.find.as_mut() else {
            return;
        };
        let changed = if document.renders_markdown() {
            let source = gpui::base::text::TextView::markdown_find_text(&text.source);
            find.bar.search([source.as_str()], 0, cx)
        } else {
            let top = -tab.scroll.0.borrow().base_handle.offset().y;
            let from = (f32::from(top) / crate::theme::LH_UI) as usize;
            find.bar.search(text.lines(), from, cx)
        };
        if changed {
            reveal_found(tab, document.renders_markdown(), true);
        }
        cx.notify();
    }

    /// ⏎ / ⇧⏎ in the preview's bar.
    fn step_preview_find(&mut self, owner: PaneIdentity, forward: bool, cx: &mut Context<Self>) {
        let Some(pane) = self.index_of(owner).map(|index| &mut self.panes[index]) else {
            return;
        };
        let Some(document) = pane.preview.document() else {
            return;
        };
        let tab = &mut pane.preview_tab;
        if tab
            .find
            .as_mut()
            .is_some_and(|find| find.bar.step(forward).is_some())
        {
            reveal_found(tab, document.renders_markdown(), false);
            cx.notify();
        }
    }

    /// Esc in the preview's bar: it closes, the preview stays, and the
    /// keyboard goes back to the Pane. The line held it inside the preview,
    /// where the focus snap (`render`) leaves the preview's own text alone,
    /// so it is let go first.
    fn close_preview_find(
        &mut self,
        owner: PaneIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.index_of(owner).map(|index| &mut self.panes[index]) {
            pane.preview_tab.find = None;
            window.blur(cx);
            cx.notify();
        }
    }
}

/// A file's lines, numbered (F-14), in the code face on the grid's cells
/// (`CH`, what the cut below counts in) — monospaced, so the numbers stand
/// in tabular columns: a `READER_NUMBER_W` column of numbers, right-aligned
/// and dim, `READER_NUMBER_GAP` before the code, the code in its syntax
/// colours (`highlight_tokens` for the path's language). The target line
/// lies on `paint::SELECTED` with its number in `TEXT_STRONG`. A line wider
/// than the preview is cut at its edge; the one holding the find's
/// occurrence starts further in when that would lie past it (`shown_from`).
/// Virtualised: only the rows in view are laid out, read from the shared
/// text without copying the rest.
fn numbered_lines(
    path: &Path,
    text: Arc<Text>,
    target: Option<u32>,
    scroll: gpui::UniformListScrollHandle,
    find: Option<&crate::find::Find>,
) -> AnyElement {
    use crate::theme::*;
    let language = ferrite_core::transcript::language_for_path(path);
    let count = text.line_count();
    // A find's occurrences: its query, and the line and ordinal it is on.
    let find = find.map(|find| {
        (
            find.query().clone(),
            find.active().map(|hit| (hit.unit, hit.ordinal)),
        )
    });
    let list = scroll.clone();
    gpui::uniform_list("preview-lines", count, move |range, _window, _cx| {
        // The cells a line shows before the preview's edge cuts it.
        let width = list.0.borrow().base_handle.bounds().size.width;
        let columns = ((f32::from(width) - READER_NUMBER_W) / CH).max(0.) as usize;
        range
            .map(|at| {
                let line = text.line(at);
                let tokens = ferrite_core::transcript::highlight_tokens(language, line);
                let highlights = pane::code(line, Some(tokens.as_slice()));
                let mut washes = Vec::new();
                let mut from = 0;
                if let Some((query, active)) = &find {
                    for (ordinal, range) in query.matches(line).into_iter().enumerate() {
                        let on = *active == Some((at, ordinal));
                        if on {
                            from = shown_from(line, &range, columns);
                        }
                        washes.push((range, if on { FIND_ACTIVE_WASH } else { FIND_WASH }));
                    }
                }
                let (line, highlights) = cut_head(
                    line.to_string(),
                    crate::find::washed(highlights, &washes),
                    from,
                );
                let number = at + 1;
                let marked = target == Some(number as u32);
                div()
                    .flex()
                    .w_full()
                    .h(px(LH_UI))
                    .whitespace_nowrap()
                    .when(marked, |row| row.bg(paint::SELECTED))
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .justify_end()
                            .w(px(READER_NUMBER_W))
                            .pr(px(READER_NUMBER_GAP))
                            .text_color(rgb(if marked { TEXT_STRONG } else { TEXT_DIM }))
                            .child(SharedString::from(number.to_string())),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_color(rgb(SYN_PLAIN))
                            .child(
                                gpui::StyledText::new(SharedString::from(line))
                                    .with_highlights(highlights),
                            ),
                    )
            })
            .collect::<Vec<_>>()
    })
    .track_scroll(&scroll)
    .flex_1()
    .min_h_0()
    .w_full()
    .py(px(HALF_ROW))
    .font_family(FONT_CODE)
    .into_any_element()
}

/// A file the preview cannot show: why, quietly, centred on the card, and
/// its original a click away — the image viewer's own note's twin.
fn unshown(why: &Unshown, path: &Path, cx: &mut Context<CockpitView>) -> AnyElement {
    use crate::theme::*;
    let (note, detail) = match why {
        Unshown::Binary => ("This is not a text file", None),
        Unshown::Unreadable(error) => ("Could not read this file", Some(error.clone())),
    };
    let original = path.to_path_buf();
    div()
        .debug_selector(|| "preview-unshown".into())
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(SPACE_2))
        .px(px(IMAGE_INSET))
        .text_color(rgb(TEXT_DIM))
        .child(note)
        .children(detail.map(|detail| {
            div()
                .text_size(px(FS_META))
                .line_height(px(LH_META))
                .text_center()
                .child(detail)
        }))
        .child(
            crate::components::form_button("preview-open-original", cx)
                .debug_selector(|| "preview-open-original".into())
                .h(px(ROW))
                .px(px(CH))
                .child(crate::components::form_label("open original", TEXT_DIM))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    crate::preview::open_original(&original, window, cx);
                }),
        )
        .into_any_element()
}

/// Where a file's line starts drawing so the occurrence the find is on
/// (`found`) shows in the `columns` cells before the preview's edge: its
/// head (0) when it fits, else the occurrence a third of the way in.
fn shown_from(text: &str, found: &std::ops::Range<usize>, columns: usize) -> usize {
    if columns == 0 || text[..found.end].chars().count() <= columns {
        return 0;
    }
    let lead = (text[..found.start].chars().count()).saturating_sub(columns / 3);
    text.char_indices().nth(lead).map_or(0, |(at, _)| at)
}

/// `text` from byte `from` on, a `…` in place of the head it skips, its
/// highlights moved with it.
fn cut_head(
    text: String,
    highlights: Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>,
    from: usize,
) -> (String, Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>) {
    if from == 0 {
        return (text, highlights);
    }
    let shift = |at: usize| at.max(from) - from + '…'.len_utf8();
    let highlights = highlights
        .into_iter()
        .filter(|(range, _)| range.end > from)
        .map(|(range, style)| (shift(range.start)..shift(range.end), style))
        .collect();
    (format!("…{}", &text[from..]), highlights)
}

/// Bring the preview's active occurrence into view: a file's line centred
/// (the list scrolls itself), or a Markdown occurrence once it has painted
/// (`settle_markdown_found`). A new query (`start_in_view`) in a Markdown
/// document is on its first occurrence until that paint says which is the
/// first in view.
fn reveal_found(tab: &mut PreviewTab, markdown: bool, start_in_view: bool) {
    let Some(find) = &mut tab.find else {
        return;
    };
    let Some(hit) = find.bar.active().copied() else {
        return;
    };
    if markdown {
        let report = find.bar.report();
        report.revealed.set(None);
        report.ahead.set(None);
        find.reveal = crate::find::REVEAL_FRAMES;
        find.start_in_view = start_in_view;
    } else {
        tab.scroll
            .scroll_to_item(hit.unit, gpui::ScrollStrategy::Center);
    }
}

/// Each frame while a Markdown reveal is due: once the active occurrence
/// has painted, a new query moves to the first occurrence in view (or the
/// first below it) and paints again; then an occurrence outside the
/// preview's view scrolls a third of the way down.
fn settle_markdown_found(find: &mut PreviewFind, scroll: &ScrollHandle, window: &mut Window) {
    if find.reveal == 0 {
        return;
    }
    let report = find.bar.report().clone();
    let Some(found) = report.revealed.get() else {
        find.reveal -= 1;
        if find.reveal > 0 {
            window.request_animation_frame();
        }
        return;
    };
    let start = std::mem::take(&mut find.start_in_view);
    let on = find.bar.active().map(|hit| hit.ordinal);
    if let Some(ahead) = report
        .ahead
        .get()
        .filter(|ahead| start && Some(*ahead) != on)
    {
        find.bar.go_to(ahead);
        report.revealed.set(None);
        find.reveal = crate::find::REVEAL_FRAMES;
        window.request_animation_frame();
        return;
    }
    find.reveal = 0;
    let view = scroll.bounds();
    if found.top() < view.top() || found.bottom() > view.bottom() {
        let goal = view.top() + view.size.height / 3.;
        let offset = scroll.offset();
        let lowest = -scroll.max_offset().y;
        let y = (offset.y - (found.top() - goal)).clamp(lowest, px(0.));
        scroll.set_offset(gpui::point(offset.x, y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file's line too long for the preview starts further in on the
    /// line the find is on, so its occurrence shows: a third of the way
    /// across, a `…` for the head, the highlights moved with the text.
    #[test]
    fn a_long_line_slides_its_occurrence_into_view() {
        let text = format!("{}needle()", "x".repeat(100));
        let found = 100..106;
        assert_eq!(shown_from(&text, &found, 120), 0, "it fits");
        let from = shown_from(&text, &found, 60);
        assert_eq!(from, 80, "a third of 60 cells before it");
        let wash = gpui::HighlightStyle::default();
        let (shown, highlights) = cut_head(text, vec![(0..4, wash), (100..106, wash)], from);
        assert_eq!(shown, format!("…{}needle()", "x".repeat(20)));
        let at = &highlights[0].0;
        assert_eq!(highlights.len(), 1, "the skipped head's highlight is gone");
        assert_eq!(&shown[at.clone()], "needle");
    }
}
