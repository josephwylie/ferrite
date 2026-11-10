//! The Panel's Preview tab (SPEC §5.6.3): a Pane's preview
//! (`ferrite_pane::preview`) read in the right Panel — a file at a line (numbered
//! lines, the target marked and centred, in syntax colour), rendered
//! Markdown, a command's output, or a note for a file it cannot show, its
//! original a click away. An image opens in the lightbox instead
//! (`cockpit/lightbox.rs`).
//!
//! Whatever opens a file in a Pane goes through `Preview::open` — the
//! cockpit's own entry points (a file link, a ⌘-clicked path, a changed
//! file, a review's line, the empty board) through `open_preview` — and the
//! cockpit sees it on its next frame (`tend_previews`): the Panel's head
//! grows a Preview tab, the file's name cut from its start, and selects it;
//! a line opened at is scrolled to the middle of the list. A Panel that was
//! hidden shows for it, and hides again when it closes. Another file opening
//! over it replaces it in place, its body fading in (`motion::swap_in`).
//! Escape in it (the `Reader` key context, deeper than the Cockpit's
//! interrupt), its × or ⌘W while it holds the keyboard closes it, and the
//! Panel goes back to its sections. ⌘F finds in it
//! (`PreviewFind`, the transcript's bar).
//!
//! While the Preview tab is the one shown the Panel is wide (`preview_w`,
//! the theme's `PANEL_PREVIEW_*`), snapping there and back with the board.
//!
//! A page (`ferrite_pane::page`: an `.html` file or a local address, opened
//! from its card in the transcript) reads live in the same card: its head
//! names it by its title, a press there opening its menu (reload, open in
//! the browser, copy its path or address), then expand — the Panel takes
//! all the board can give — and ×. Where the engine can't draw it (no
//! Chromium in this build, or an address) the body is the "can't show"
//! note, open in browser a click away.
//!
//! Each Pane keeps its own preview (`PaneView::preview_tab`), and the Panel
//! shows the focused Pane's: focus moving to another Thread leaves the
//! preview where it is, and coming back finds it as it was. A draft, which
//! has no Panel of its own, shows the Panel for its preview alone (the
//! Preview tab, no other), and its first send keeps it as it was.

use super::*;
use ferrite_pane::page::Shown;
use ferrite_pane::preview::{
    head_door, head_suffix, head_title, reading_head, Body, Text, Unshown,
};
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
    /// under this key (`Preview::opened`'s).
    fade: Option<u64>,
    /// The numbered lines' own scroll, kept so a line opened at can be
    /// scrolled into view.
    pub(crate) scroll: gpui::UniformListScrollHandle,
    /// A rendered Markdown document's own scroll.
    pub(crate) markdown_scroll: ScrollHandle,
    /// The Markdown document's native text.
    rich: ferrite_pane::rich::TextCache,
    /// The find bar, while it is open.
    pub(crate) find: Option<PreviewFind>,
    annotation_image: Option<AnnotationImage>,
}

struct AnnotationImage {
    annotation: ferrite_core::browser_annotations::Annotation,
    opened: Option<u64>,
    image: std::sync::mpsc::Receiver<std::io::Result<PathBuf>>,
}

impl Default for PreviewTab {
    fn default() -> Self {
        Self {
            seen: None,
            selected: false,
            fade: None,
            scroll: gpui::UniformListScrollHandle::new(),
            markdown_scroll: ScrollHandle::new(),
            rich: ferrite_pane::rich::TextCache::default(),
            find: None,
            annotation_image: None,
        }
    }
}

/// Find in the preview (⌘F when the preview took the last press or holds
/// the keyboard; the bar is the transcript's, `ferrite_pane::find`): a file's
/// lines, or a rendered Markdown document's text.
pub(crate) struct PreviewFind {
    bar: ferrite_pane::find::Find,
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

    /// `Overview` selected for `thread`'s Pane: its preview stays open,
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
    /// sections — or, shown for the preview alone, hides.
    pub(in crate::cockpit) fn close_preview(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(index) {
            pane.preview.close();
            self.panel.expanded = false;
            self.focus_pane(index);
            cx.notify();
        }
    }

    /// Each frame, before the board is measured: a preview that opened selects
    /// the Preview tab (marked to fade in, after the first frame) and
    /// scrolls its line to the middle of the list; one that closed hands
    /// the Panel back to its sections. A find bar closes with
    /// its document, and a Markdown reveal that is due settles.
    pub(in crate::cockpit) fn tend_previews(&mut self, window: &mut Window) {
        for pane in &mut self.panes {
            for annotation in pane.preview.take_annotation_requests() {
                let image = pane
                    .thread()
                    .ok_or_else(|| {
                        std::io::Error::other(
                            "Open the owning Thread to review this browser capture",
                        )
                    })
                    .and_then(|thread| {
                        self.cockpit
                            .browser_annotation_image(thread, annotation.clone())
                    });
                match image {
                    Ok(image) => {
                        pane.preview.loading_annotation(annotation.clone(), window);
                        pane.preview_tab.annotation_image = Some(AnnotationImage {
                            annotation,
                            image,
                            opened: pane.preview.opened().map(|(open, _)| open),
                        });
                    }
                    Err(error) => {
                        pane.preview
                            .annotation_error(&annotation, error.to_string(), window)
                    }
                }
            }
            if let Some(pending) = pane.preview_tab.annotation_image.take() {
                if pane.preview.opened().map(|(open, _)| open) == pending.opened {
                    match pending.image.try_recv() {
                        Ok(Ok(image)) => pane.preview.open_annotation(pending.annotation, image, window),
                        Ok(Err(error)) => pane.preview.annotation_error(&pending.annotation, format!("The retained capture could not be opened: {error}. Reopen the annotation to try again."), window),
                        Err(std::sync::mpsc::TryRecvError::Empty) => pane.preview_tab.annotation_image = Some(pending),
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => pane.preview.annotation_error(&pending.annotation, "The retained capture could not be loaded. Reopen the annotation to try again.".into(), window),
                    }
                }
            }
        }
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
        let document = self.panes[self.index_of(owner)?].preview.document()?;
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
        use ferrite_ui::theme::*;
        let id = gpui::ElementId::from("panel-tab-Preview");
        let key = ferrite_ui::pointer::hover_key(&id);
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
                    face.hover_carried(key).text_color(rgb(INK_STRONG))
                } else {
                    face.text_color(rgb(INK_STRONG))
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
    /// the board's cards' twin (`R_CARD` on `CARD`, ringed), its top and
    /// foot level with theirs and its head level with the Pane head beside
    /// it. The head (`reading_head`) reads the file's path from the
    /// Thread's workspace in `LINK_INK` and its `:line` dim — then, for a
    /// file, copy its path
    /// and open it in its app, and the ×. A press lands on the Pane that
    /// opened it; escape closes it.
    pub(in crate::cockpit) fn panel_preview(
        &self,
        owner: PaneIdentity,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let Some(index) = self.index_of(owner) else {
            return div().into_any_element();
        };
        let pane = &self.panes[index];
        let Some(document) = pane.preview.document() else {
            return div().into_any_element();
        };
        let tab = &pane.preview_tab;
        let find = tab.find.as_ref();
        let rich = document.rich_source();
        let body = match &document.body {
            Body::Text { .. } if rich.is_some() => {
                tab.rich.file_context(document.path.parent(), &pane.preview);
                let marks = find.and_then(|find| find.bar.marks(0));
                div()
                    .id("preview-markdown")
                    .debug_selector(|| "preview-markdown".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&tab.markdown_scroll)
                    .px(px(TX_INSET_SOLO))
                    .pt(px(BODY_PAD_T))
                    .pb(px(HALF_ROW))
                    // A reading document (Markdown, prose) reads at the
                    // transcript's reading size.
                    .text_size(px(FS_PROSE))
                    .line_height(px(LH_PROSE))
                    .child(
                        ferrite_pane::rich::Markdown::new(
                            format!("document-{}", document.path.display()),
                            rich.clone().unwrap_or_default(),
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
            )
            .into_any_element(),
            Body::Browser => self.browser_body(pane.thread(), cx),
            Body::BrowserAnnotation { annotation, image } => div()
                .debug_selector(|| "browser-annotation-review".into())
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap(px(ferrite_ui::theme::SPACE_2))
                .child(
                    div()
                        .text_size(px(FS_META))
                        .text_color(rgb(INK_SUBTLE))
                        .child(annotation.capture().url.clone()),
                )
                .when(!annotation.comment().is_empty(), |review| {
                    review.child(
                        div()
                            .text_color(rgb(INK_STRONG))
                            .child(annotation.comment().to_owned()),
                    )
                })
                .child(match image {
                    Some(image) => {
                        ferrite_pane::annotation_editor::annotation_stage(annotation, image.clone())
                    }
                    None => div().child("Loading capture…").into_any_element(),
                })
                .into_any_element(),
            Body::Unshown(why) => unshown(why, &document.path, cx),
            Body::Page(page) => match page.shown(cx) {
                Shown::Live(web) => div()
                    .id("preview-page")
                    .debug_selector(|| "preview-page".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(web)
                    .into_any_element(),
                Shown::Starting => div().flex_1().into_any_element(),
                Shown::Unshown(why) => {
                    let url = page.source().url();
                    cant_show(
                        "This page can't show here",
                        Some(why),
                        "open in browser",
                        move |_, cx| cx.open_url(&url),
                        cx,
                    )
                }
            },
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
                ferrite_ui::motion::swap_in(SharedString::from(format!("preview-swap-{key}")), body)
                    .into_any_element()
            }
            None => body.into_any_element(),
        };
        let page = document.page().map(|page| page.source().clone());
        let title = match &page {
            Some(page) => page.title(),
            None => self.preview_title(pane, &document),
        };
        let label = match page {
            Some(_) => self.page_title_menu(owner, title.clone(), cx),
            None => head_title(title.clone())
                .children(document.line().map(|line| head_suffix(format!(":{line}")))),
        };
        // A file (not a command's output, which no file holds, nor a page,
        // whose menu does it) can have its path copied, as the head reads
        // it, and open in its own app.
        let file = match &document.body {
            Body::BrowserAnnotation {
                annotation,
                image: Some(image),
            } => Some((image.clone(), annotation.original().display().to_string())),
            _ => (document.name.is_none() && document.page().is_none())
                .then(|| (document.path.clone(), title.clone())),
        };
        let expand = (document.page().is_some()
            || matches!(
                document.body,
                Body::Browser | Body::BrowserAnnotation { .. }
            ))
        .then(|| self.expand_door(cx));
        let doors = file
            .map(|(original, shown)| {
                [
                    div()
                        .debug_selector(|| "preview-copy-path".into())
                        .child(
                            ferrite_ui::components::copy_button("preview-copy-path", shown)
                                .tooltip_text("Copy path")
                                .door(ICON_BUTTON, ICON_BUTTON_GLYPH, R_CONTROL),
                        )
                        .into_any_element(),
                    head_door(
                        "open-original-image",
                        "preview-original".into(),
                        ferrite_ui::icons::EXTERNAL,
                    )
                    .tooltip(ferrite_ui::menu::tooltip("Open in its app"))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        ferrite_pane::preview::open_original(&original, window, cx);
                    })
                    .into_any_element(),
                ]
            })
            .into_iter()
            .flatten()
            .chain(expand)
            .chain([head_door(
                "preview-close",
                "preview-close".into(),
                ferrite_ui::icons::CLOSE,
            )
            .tooltip(ferrite_ui::menu::tooltip_with_key(
                "Close the preview",
                ferrite_ui::components::bound_chord_in(
                    "reader::Close",
                    Some(ferrite_ui::keymap::READER),
                ),
            ))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                if let Some(index) = view.index_of(owner) {
                    view.close_preview(index, cx);
                }
            }))
            .into_any_element()]);
        div()
            .debug_selector(|| "panel-preview".into())
            .track_focus(pane.preview.focus_handle())
            .key_context(ferrite_ui::keymap::READER)
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .rounded(px(R_CARD))
            .bg(paint::CARD)
            .whitespace_normal()
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .child(reading_head(label, doors))
            .child(body)
            .child(pane::card_ring(paint::CARD_RING))
            .on_action(cx.listener(move |view, _: &Close, _, cx| {
                if let Some(index) = view.index_of(owner) {
                    view.close_preview(index, cx);
                }
            }))
            .on_action(
                cx.listener(move |view, _: &ferrite_pane::find::Next, _, cx| {
                    view.step_preview_find(owner, true, cx)
                }),
            )
            .on_action(
                cx.listener(move |view, _: &ferrite_pane::find::Previous, _, cx| {
                    view.step_preview_find(owner, false, cx)
                }),
            )
            .on_action(
                cx.listener(move |view, _: &ferrite_pane::find::Close, window, cx| {
                    view.close_preview_find(owner, window, cx)
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

    /// The page Pane `owner`'s preview shows, if it shows one.
    pub(in crate::cockpit) fn page_source(&self, owner: PaneIdentity) -> Option<PageSource> {
        let pane = self.panes.get(self.index_of(owner)?)?;
        Some(pane.preview.document()?.page()?.source().clone())
    }

    /// A page head's title: its name and a ▾, a press opening its menu
    /// (`MenuTarget::Page`) under it.
    fn page_title_menu(&self, owner: PaneIdentity, title: String, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let id = gpui::ElementId::from("preview-page-title");
        let key = ferrite_ui::pointer::hover_key(&id);
        let drawn: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>> = Default::default();
        let measured = drawn.clone();
        div().flex().min_w_0().child(
            div()
                .id(id)
                .debug_selector(|| "preview-page-title".into())
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(SPACE_1))
                .h(px(ICON_BUTTON))
                .px(px(SPACE_1))
                .rounded(px(R_CONTROL))
                .relative()
                .hover_raised(key)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    gpui::canvas(
                        move |bounds, _, _| measured.set(Some(bounds)),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(W_LABEL)
                        .text_color(rgb(INK_STRONG))
                        .child(title),
                )
                .child(ferrite_ui::icons::icon(
                    ferrite_ui::icons::CHEVRON_DOWN,
                    FS_META,
                    INK_SUBTLE,
                ))
                .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    // Under the title, its left edge on the title's.
                    let at = drawn.get().map_or(event.position(), |title| {
                        gpui::point(title.left(), title.bottom() + px(MENU_UNDER_GAP))
                    });
                    view.open_context_menu_at(
                        MenuTarget::Page(owner),
                        at,
                        gpui::Anchor::TopLeft,
                        None,
                        cx,
                    );
                })),
        )
    }

    /// Expand the preview to all the board can give, or back.
    fn expand_door(&self, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.panel.expanded;
        head_door(
            "preview-expand",
            "preview-expand".into(),
            if expanded {
                ferrite_ui::icons::MINIMIZE
            } else {
                ferrite_ui::icons::MAXIMIZE
            },
        )
        .tooltip(ferrite_ui::menu::tooltip(if expanded {
            "Collapse"
        } else {
            "Expand"
        }))
        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.panel.expanded = !view.panel.expanded;
            cx.notify();
        }))
        .into_any_element()
    }

    /// A page's menu verbs (`MenuTarget::Page`).
    pub(in crate::cockpit) fn run_page_verb(
        &mut self,
        owner: PaneIdentity,
        verb: MenuVerb,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index_of(owner) else {
            return;
        };
        let Some(page) = self.panes[index]
            .preview
            .document()
            .and_then(|document| document.page().cloned())
        else {
            return;
        };
        match verb {
            MenuVerb::Reload => page.reload(cx),
            MenuVerb::OpenInBrowser => cx.open_url(&page.source().url()),
            MenuVerb::CopyPath => {
                let copied = match page.source() {
                    PageSource::File(path) => path.display().to_string(),
                    PageSource::Address(address) => address.clone(),
                };
                cx.write_to_clipboard(ClipboardItem::new_string(copied));
            }
            _ => {}
        }
        cx.notify();
    }

    /// What the head reads for `document`: a command's name, else the
    /// path from the Thread's workspace, else from `~`: the tab above
    /// already reads its name.
    fn preview_title(&self, pane: &PaneView, document: &ferrite_pane::preview::Document) -> String {
        if let Some(name) = &document.name {
            return name.clone();
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
                bar: ferrite_pane::find::Find::new(cx, move |view: &mut Self, cx| {
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
        let rich = document.rich_source();
        let changed = if let Some(rich) = &rich {
            let source = gpui::base::text::TextView::markdown_find_text(rich);
            find.bar.search([source.as_str()], 0, cx)
        } else {
            let top = -tab.scroll.0.borrow().base_handle.offset().y;
            let from = (f32::from(top) / ferrite_ui::theme::LH_UI) as usize;
            find.bar.search(text.lines(), from, cx)
        };
        if changed {
            reveal_found(tab, &text, rich.is_some(), true);
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
            if let Some(text) = document.text() {
                reveal_found(tab, text, document.rich_source().is_some(), false);
            }
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
/// (`CH`) — monospaced, so the numbers stand in tabular columns: a
/// `READER_NUMBER_W` column of numbers, right-aligned and dim,
/// `READER_NUMBER_GAP` before the code, the code in its syntax colours
/// (`highlight_tokens` for the path's language). The target line lies on
/// `paint::SELECTED` with its number in `INK_STRONG`. A line wider than
/// the preview is never cut: the list scrolls sideways as wide as its
/// widest line, the numbers standing still over the code on the card's
/// own ground while it slides under them; a find's occurrence past the
/// edge scrolls into view (`reveal_found`). Virtualised: only the rows in
/// view are laid out, read from the shared text without copying the rest.
fn numbered_lines(
    path: &Path,
    text: Arc<Text>,
    target: Option<u32>,
    scroll: gpui::UniformListScrollHandle,
    find: Option<&ferrite_pane::find::Find>,
) -> gpui::UniformList {
    use ferrite_ui::theme::*;
    let language = ferrite_core::transcript::language_for_path(path);
    let count = text.line_count();
    let widest = text.widest_line();
    // A find's occurrences: its query, and the line and ordinal it is on.
    let find = find.map(|find| {
        (
            find.query().clone(),
            find.active().map(|hit| (hit.unit, hit.ordinal)),
        )
    });
    let list = scroll.clone();
    gpui::uniform_list("preview-lines", count, move |range, _window, _cx| {
        // How far the code has slid left: the numbers stay put by as much.
        let slid = -list.0.borrow().base_handle.offset().x;
        range
            .map(|at| {
                let line = text.line(at);
                let tokens = ferrite_core::transcript::highlight_tokens(language, line);
                let highlights = pane::code(line, Some(tokens.as_slice()));
                let mut washes = Vec::new();
                if let Some((query, active)) = &find {
                    for (ordinal, range) in query.matches(line).into_iter().enumerate() {
                        let on = *active == Some((at, ordinal));
                        washes.push((range, if on { FIND_ACTIVE_WASH } else { FIND_WASH }));
                    }
                }
                let highlights = ferrite_pane::find::washed(highlights, &washes);
                let number = at + 1;
                let marked = target == Some(number as u32);
                div()
                    .relative()
                    .flex()
                    .w_full()
                    .h(px(LH_UI))
                    .pl(px(READER_NUMBER_W))
                    .whitespace_nowrap()
                    .when(marked, |row| row.bg(paint::SELECTED))
                    .child(
                        div()
                            .flex_shrink_0()
                            .pr(px(CH))
                            .text_color(rgb(SYN_PLAIN))
                            .child(
                                gpui::StyledText::new(SharedString::from(line.to_string()))
                                    .with_highlights(highlights),
                            ),
                    )
                    // Laid after the code, so it covers what slides under it.
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left(slid)
                            .flex()
                            .justify_end()
                            .w(px(READER_NUMBER_W))
                            .h(px(LH_UI))
                            .pr(px(READER_NUMBER_GAP))
                            .text_size(px(FS_META))
                            .bg(if marked { paint::SELECTED } else { paint::CARD })
                            .text_color(rgb(if marked { INK_STRONG } else { INK_SUBTLE }))
                            .child(SharedString::from(number.to_string())),
                    )
            })
            .collect::<Vec<_>>()
    })
    .with_horizontal_sizing_behavior(gpui::ListHorizontalSizingBehavior::Unconstrained)
    .with_width_from_item(Some(widest))
    .track_scroll(&scroll)
    .flex_1()
    .min_h_0()
    .w_full()
    .py(px(HALF_ROW))
    .font_family(FONT_CODE)
    .text_size(px(FS_READER_CODE))
    .line_height(px(LH_UI))
}

/// A file the preview cannot show: why, quietly, centred on the card, and
/// its original a click away — the image viewer's own note's twin.
fn unshown(why: &Unshown, path: &Path, cx: &mut Context<CockpitView>) -> AnyElement {
    let (note, detail) = match why {
        Unshown::Binary => ("This is not a text file", None),
        Unshown::Unreadable(error) => ("Could not read this file", Some(error.clone())),
    };
    let original = path.to_path_buf();
    cant_show(
        note,
        detail,
        "open original",
        move |window, cx| ferrite_pane::preview::open_original(&original, window, cx),
        cx,
    )
}

/// The "can't show" note: what, why under it, and the one door that shows
/// it elsewhere.
fn cant_show(
    note: &'static str,
    detail: Option<SharedString>,
    door: &'static str,
    open: impl Fn(&mut Window, &mut gpui::App) + 'static,
    cx: &mut Context<CockpitView>,
) -> AnyElement {
    use ferrite_ui::theme::*;
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
        .text_color(rgb(INK_SUBTLE))
        .child(note)
        .children(detail.map(|detail| {
            div()
                .text_size(px(FS_META))
                .line_height(px(LH_META))
                .text_center()
                .child(detail)
        }))
        .child(
            ferrite_ui::components::form_button("preview-open-original", cx)
                .debug_selector(|| "preview-open-original".into())
                .h(px(ROW))
                .px(px(CH))
                .child(ferrite_ui::components::form_label(door, INK_SUBTLE))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    open(window, cx);
                }),
        )
        .into_any_element()
}

/// Bring the preview's active occurrence into view: a file's line centred
/// (the list scrolls itself) and, when the occurrence lies past either
/// side of the view, the code slid so it starts a third of the way in; or
/// a Markdown occurrence once it has painted (`settle_markdown_found`). A
/// new query (`start_in_view`) in a Markdown document is on its first
/// occurrence until that paint says which is the first in view.
fn reveal_found(tab: &mut PreviewTab, text: &Text, markdown: bool, start_in_view: bool) {
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
        find.reveal = ferrite_pane::find::REVEAL_FRAMES;
        find.start_in_view = start_in_view;
        return;
    }
    tab.scroll
        .scroll_to_item(hit.unit, gpui::ScrollStrategy::Center);
    if hit.unit >= text.line_count() {
        return;
    }
    let line = text.line(hit.unit);
    let Some(found) = find.bar.query().matches(line).into_iter().nth(hit.ordinal) else {
        return;
    };
    let handle = &tab.scroll.0.borrow().base_handle;
    let offset = handle.offset();
    let view = f32::from(handle.bounds().size.width) - ferrite_ui::theme::READER_NUMBER_W;
    let x = slide_to(line, &found, -f32::from(offset.x), view);
    handle.set_offset(gpui::point(px(-x), offset.y));
}

/// How far a file's line slides left so its occurrence `found` shows in a
/// `view` wide: where it is now (`slid`) while the occurrence lies inside,
/// else the occurrence a third of the way in (never past the line's head).
fn slide_to(line: &str, found: &std::ops::Range<usize>, slid: f32, view: f32) -> f32 {
    let cell = ferrite_ui::theme::CH;
    let start = line[..found.start].chars().count() as f32 * cell;
    let end = line[..found.end].chars().count() as f32 * cell;
    if start >= slid && end <= slid + view {
        return slid;
    }
    (start - view / 3.).max(0.)
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
        find.reveal = ferrite_pane::find::REVEAL_FRAMES;
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

    /// A find's occurrence past the view's edge slides a long line so it
    /// starts a third of the way in; one already in view leaves the line
    /// where it is.
    #[test]
    fn a_long_line_slides_its_occurrence_into_view() {
        let cell = ferrite_ui::theme::CH;
        let text = format!("{}needle()", "x".repeat(100));
        let found = 100..106;
        assert_eq!(slide_to(&text, &found, 0., 120. * cell), 0., "it fits");
        assert_eq!(
            slide_to(&text, &found, 0., 60. * cell),
            80. * cell,
            "a third of 60 cells before it"
        );
        assert_eq!(
            slide_to(&text, &found, 90. * cell, 60. * cell),
            90. * cell,
            "already in view: it stays"
        );
        assert_eq!(
            slide_to(&text, &(0..1), 90. * cell, 60. * cell),
            0.,
            "back to the head"
        );
    }

    /// The reader's code and head sit on the type scale's tokens: code at
    /// the transcript's code size on the grid's row, the head at the UI
    /// size (the owner's: the reader read larger than the transcript).
    #[test]
    fn the_readers_code_and_head_are_set_in_the_scales_tokens() {
        use ferrite_ui::theme::{FONT_CODE, FS_PROSE, FS_READER_CODE, FS_UI, LH_UI, TX_BLOCK_STEP};
        assert_eq!(FS_READER_CODE, FS_PROSE - TX_BLOCK_STEP, "the fences' size");
        let text = Arc::new(Text::new("fn main() {}\n".into()));
        let mut lines = numbered_lines(
            Path::new("main.rs"),
            text,
            None,
            gpui::UniformListScrollHandle::new(),
            None,
        );
        let style = lines.style();
        assert_eq!(style.text.font_family.as_deref(), Some(FONT_CODE));
        assert_eq!(style.text.font_size, Some(px(FS_READER_CODE).into()));
        assert_eq!(style.text.line_height, Some(px(LH_UI).into()));
        let mut head = reading_head(div(), []);
        assert_eq!(head.style().text.font_size, Some(px(FS_UI).into()));
    }
}
