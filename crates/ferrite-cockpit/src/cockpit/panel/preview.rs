//! The Panel's File Surface (SPEC §5.6.3, ADR 0039): a Pane's preview
//! (`ferrite_pane::preview`) read in the right Panel — a file at a line (numbered
//! lines, the target marked and centred, in syntax colour), rendered
//! Markdown, a command's output, or a note for a file it cannot show, its
//! original a click away. An image opens in the lightbox instead
//! (`cockpit/lightbox.rs`).
//!
//! Whatever opens a file in a Pane goes through `Preview::open` — the
//! cockpit's own entry points (a file link, a ⌘-clicked path, a changed
//! file, a review's line, the empty board) through `open_preview` — and the
//! cockpit sees it on its next frame (`tend_previews`): the document goes
//! through the Panel's one door as a `File` Surface (ADR 0039), one tab
//! per file, its name for a title. A file already in a tab brings that tab
//! forward; another file gets a tab of its own, the first kept. Pressing a
//! File tab shows its document again as it was last read. A line opened at
//! is scrolled to the middle of the list. Escape in it (the `Reader` key
//! context, deeper than the Cockpit's interrupt), its tab's × or ⌘W while
//! it holds the keyboard closes it. ⌘F finds in it (`PreviewFind`, the
//! transcript's bar).
//!
//! While a File shows the Panel is at least a reading width (`panel`'s
//! floor, the theme's `PANEL_PREVIEW_*`).
//!
//! A page (`ferrite_pane::page`: an `.html` file or a local address, opened
//! from its card in the transcript) reads live in the same card: its head
//! names it by its title, a press there opening its menu (reload, open in
//! the browser, copy its path or address). Where the engine can't draw it (no
//! Chromium in this build, or an address) the body is the "can't show"
//! note, open in browser a click away.
//!
//! Each Pane keeps its own preview, and the Panel shows the focused Pane's:
//! focus moving to another Thread leaves the preview where it is, and
//! coming back finds it as it was. A draft, which has no Panel of its own,
//! shows the Panel for its preview alone (no tabs), and its first send
//! keeps it as it was.
//!
//! What the Surface keeps of each Pane's preview lives in a view of its
//! own (ADR 0041), `FileSurfaceView`: each File tab's document as last
//! read, the fade, the scrolls, the find bar, a Browser annotation's
//! capture on its way, and the Pane whose preview took the last press. It
//! has no render of its own: the root has it draw the card, handing it the
//! Pane's preview, its checkout and the Browser's body with
//! `file_surface_inputs`. What it asks of the window it says as a
//! `FileSurfaceEvent`. The document itself is the Pane's (`Preview`), and
//! which Surface the Panel shows is the root's.

use super::*;
use ferrite_pane::page::Shown;
pub(crate) use ferrite_pane::preview::BROWSER_PATH;
use ferrite_pane::preview::{
    head_suffix, head_title, reading_head, Body, Document, Preview, Text, Unshown,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

gpui::actions!(reader, [Close]);

/// What the File Surface keeps of one Pane's preview.
pub(crate) struct PreviewTab {
    /// The Pane's preview, the handle its transcript and Composer share.
    preview: Preview,
    /// The open last tended (`Preview::opened`).
    seen: Option<u64>,
    /// Each File tab's document as last read, by path, so pressing its tab
    /// shows it again (a command's output has no file to read).
    documents: HashMap<PathBuf, Document>,
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

pub(in crate::cockpit) struct AnnotationImage {
    annotation: ferrite_core::browser_annotations::Annotation,
    opened: Option<u64>,
    image: ferrite_core::cockpit::Ask<PathBuf>,
}

impl PreviewTab {
    fn new(preview: Preview) -> Self {
        Self {
            preview,
            seen: None,
            documents: Default::default(),
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
    pub(in crate::cockpit) bar: ferrite_pane::find::Find,
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
    pub(in crate::cockpit) fn new(bar: ferrite_pane::find::Find, path: PathBuf) -> Self {
        Self {
            bar,
            path,
            reveal: 0,
            start_in_view: false,
        }
    }

    /// Have the active occurrence scrolled into view once it paints; a new
    /// query (`start_in_view`) starts on the first occurrence in view.
    pub(in crate::cockpit) fn reveal_when_painted(&mut self, start_in_view: bool) {
        let report = self.bar.report();
        report.revealed.set(None);
        report.ahead.set(None);
        self.reveal = ferrite_pane::find::REVEAL_FRAMES;
        self.start_in_view = start_in_view;
    }
}

#[cfg(test)]
impl PreviewFind {
    /// What the bar reads, and where a Markdown occurrence last painted.
    pub(crate) fn state(&self) -> (Option<SharedString>, Option<gpui::Bounds<Pixels>>) {
        (self.bar.count(), self.bar.report().revealed.get())
    }
}

/// What the File Surface keeps of every Pane's preview, by Pane.
#[derive(Default)]
pub(crate) struct FileSurfaceView {
    tabs: HashMap<PaneIdentity, PreviewTab>,
    /// The Pane whose preview took the last press: ⌘F finds there. A
    /// press hands the keyboard back to the Pane, so focus cannot say it.
    pressed: Option<PaneIdentity>,
    /// The first frame has been tended: a preview open by then is simply
    /// there, with no fade.
    tended: bool,
}

/// What the card asks of the window.
pub(in crate::cockpit) enum FileSurfaceEvent {
    /// Escape in the card: close it, the keyboard back to its Pane.
    Close(PaneIdentity),
    /// A press on the card: the keyboard goes to the Pane that opened it.
    Pressed(PaneIdentity),
    /// A page head's title: its menu, under the title at the point.
    PageMenu(PaneIdentity, gpui::Point<Pixels>),
}

impl EventEmitter<FileSurfaceEvent> for FileSurfaceView {}

/// What the card draws of the window for one Pane.
pub(in crate::cockpit) struct FileSurfaceInputs {
    owner: PaneIdentity,
    preview: Preview,
    /// The Thread's checkout, which a file's head reads its path from.
    workspace: Option<PathBuf>,
    /// The live browser's body, while the document is the Browser.
    browser: Option<AnyElement>,
}

/// What a frame's tend found.
pub(in crate::cockpit) struct Tended {
    /// Each document a Thread's preview opened, as the Surface it is.
    opened: Vec<(ThreadId, SurfaceId)>,
    /// A Browser annotation's capture is still being read.
    waiting: bool,
}

impl FileSurfaceView {
    /// What it keeps of `owner`'s preview, made the first time it is asked.
    fn tab(&mut self, owner: PaneIdentity, preview: &Preview) -> &mut PreviewTab {
        self.tabs
            .entry(owner)
            .or_insert_with(|| PreviewTab::new(preview.clone()))
    }

    /// Every Pane `keep` refuses takes what it kept with it.
    pub(in crate::cockpit) fn retain_panes(&mut self, keep: impl Fn(PaneIdentity) -> bool) {
        self.tabs.retain(|owner, _| keep(*owner));
    }

    /// A draft's first send made it `thread`'s Pane: it keeps its preview.
    pub(in crate::cockpit) fn adopt(&mut self, draft: PaneIdentity, thread: PaneIdentity) {
        if let Some(tab) = self.tabs.remove(&draft) {
            self.tabs.insert(thread, tab);
        }
    }

    /// The Pane whose preview took the last press.
    pub(in crate::cockpit) fn pressed(&self) -> Option<PaneIdentity> {
        self.pressed
    }

    /// Every press forgets the last; one in a preview marks it again.
    pub(in crate::cockpit) fn forget_press(&mut self) {
        self.pressed = None;
    }

    /// `owner`'s File tab `path`'s document, as last read.
    pub(in crate::cockpit) fn document(&self, owner: PaneIdentity, path: &Path) -> Option<&Document> {
        self.tabs.get(&owner)?.documents.get(path)
    }

    /// `owner`'s File tab `path` closed: its document is forgotten.
    pub(in crate::cockpit) fn forget_document(&mut self, owner: PaneIdentity, path: &Path) {
        if let Some(tab) = self.tabs.get_mut(&owner) {
            tab.documents.remove(path);
        }
    }

    /// The scroll the paging keys move in `owner`'s preview: a rendered
    /// document's while it scrolls, else the numbered lines'.
    pub(in crate::cockpit) fn reader_scroll(&self, owner: PaneIdentity) -> Option<ScrollHandle> {
        let tab = self.tabs.get(&owner)?;
        Some(if tab.markdown_scroll.max_offset().y > px(0.) {
            tab.markdown_scroll.clone()
        } else {
            tab.scroll.0.borrow().base_handle.clone()
        })
    }

    /// A Browser annotation's capture is being read for `owner`'s preview,
    /// which shows it loading: it opens once read, if nothing opened since.
    pub(in crate::cockpit) fn wait_annotation(
        &mut self,
        owner: PaneIdentity,
        preview: &Preview,
        annotation: ferrite_core::browser_annotations::Annotation,
        image: ferrite_core::cockpit::Ask<PathBuf>,
    ) {
        let opened = preview.opened().map(|(open, _)| open);
        self.tab(owner, preview).annotation_image = Some(AnnotationImage {
            annotation,
            opened,
            image,
        });
    }

    /// Each frame, for every Pane's preview: a capture read opens; a
    /// document that opened is kept for its File tab, marked to fade in
    /// (after the first frame) and scrolled to its line; a find bar closes
    /// with its document, and a Markdown reveal that is due settles.
    pub(in crate::cockpit) fn tend(
        &mut self,
        panes: &[(PaneIdentity, Preview)],
        window: &mut Window,
    ) -> Tended {
        let mut tended = Tended {
            opened: Vec::new(),
            waiting: false,
        };
        let first = !std::mem::replace(&mut self.tended, true);
        for (owner, preview) in panes {
            let thread = owner.thread();
            let tab = self.tab(*owner, preview);
            if let Some(mut pending) = tab.annotation_image.take() {
                if preview.opened().map(|(open, _)| open) == pending.opened {
                    match pending.image.answer() {
                        Some(Ok(image)) => preview.open_annotation(pending.annotation, image, window),
                        Some(Err(error)) if error.kind() == std::io::ErrorKind::NotConnected => preview.annotation_error(&pending.annotation, "The retained capture could not be loaded. Reopen the annotation to try again.".into(), window),
                        Some(Err(error)) => preview.annotation_error(&pending.annotation, format!("The retained capture could not be opened: {error}. Reopen the annotation to try again."), window),
                        None => {
                            tab.annotation_image = Some(pending);
                            tended.waiting = true;
                        }
                    }
                }
            }
            let opened = preview.opened();
            let document = preview.document();
            match opened {
                Some((open, key)) if tab.seen != Some(open) => {
                    if let (Some(thread), Some(document)) = (thread, &document) {
                        tab.documents.insert(document.path.clone(), document.clone());
                        tended.opened.push((thread, document_surface(document)));
                    }
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
                None if tab.seen.is_some() => tab.fade = None,
                _ => {}
            }
            tab.seen = opened.map(|(open, _)| open);
            // A draft's preview, kept through its first send: the new
            // Thread takes it as its File tab.
            if let (Some(thread), Some(document)) = (thread, &document) {
                if !tab.documents.contains_key(&document.path) {
                    tab.documents.insert(document.path.clone(), document.clone());
                    tended.opened.push((thread, document_surface(document)));
                }
            }
            let Some(find) = &mut tab.find else {
                continue;
            };
            if document.is_none_or(|document| document.path != find.path) {
                tab.find = None;
                continue;
            }
            settle_markdown_found(find, &tab.markdown_scroll, window);
        }
        tended
    }

    /// The File Surface's body: the preview as a card on the Panel's frame,
    /// the board's cards' twin (`R_CARD` on `CARD`, ringed), its top and
    /// foot level with theirs and its head level with the Pane head beside
    /// it. The head (`reading_head`) reads the file's path from the
    /// Thread's workspace in `LINK_INK` and its `:line` dim — then, for a
    /// file, copy its path
    /// and open it in its app, and the ×. A press lands on the Pane that
    /// opened it; escape closes it.
    pub(in crate::cockpit) fn card(
        &mut self,
        inputs: FileSurfaceInputs,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let FileSurfaceInputs {
            owner,
            preview,
            workspace,
            browser,
        } = inputs;
        let Some(document) = preview.document() else {
            return div().into_any_element();
        };
        let tab = self.tab(owner, &preview);
        let find = tab.find.as_ref();
        let rich = document.rich_source();
        let body = match &document.body {
            Body::Text { .. } if rich.is_some() => {
                tab.rich.file_context(document.path.parent(), &preview);
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
            Body::Browser => browser.unwrap_or_else(|| div().into_any_element()),
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
                .child(ferrite_pane::annotation_editor::annotation_context(annotation))
                .child(match image {
                    Some(image) => {
                        ferrite_pane::annotation_editor::annotation_stage(annotation, image.clone())
                    }
                    None if annotation.original().is_none() => div().into_any_element(),
                    None => div().child("Loading capture…").into_any_element(),
                })
                .into_any_element(),
            Body::Unshown(why) => unshown(why, &document.path, owner, cx),
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
                        vec![("open in browser", Box::new(move |_, cx| cx.open_url(&url)))],
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
        // The browser's page strip is its own head, under the panel's
        // Browser tab.
        let head = (!matches!(document.body, Body::Browser)).then(|| {
            let page = document.page().map(|page| page.source().clone());
            let title = match &page {
                Some(page) => page.title(),
                None => preview_title(workspace.as_deref(), &document),
            };
            let label = match page {
                Some(_) => page_title_menu(owner, title.clone(), cx),
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
                } => annotation.original().map(|original| (image.clone(), original.display().to_string())),
                _ => (document.name.is_none() && document.page().is_none())
                    .then(|| (document.path.clone(), title.clone())),
            };
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
                        ferrite_ui::components::icon_button(
                            "open-original-image",
                            ferrite_ui::icons::EXTERNAL,
                            ferrite_ui::components::IconSize::Standard,
                            "Open in its app",
                            cx,
                        )
                        .tip("Open in its app")
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            ferrite_pane::preview::open_original(&original, window, cx);
                        })
                        .into_any_element(),
                    ]
                })
                .into_iter()
                .flatten();
            reading_head(label, doors)
        });
        div()
            .debug_selector(|| "panel-preview".into())
            .track_focus(preview.focus_handle())
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
            .children(head)
            .child(body)
            .child(pane::card_ring(paint::CARD_RING))
            .on_action(cx.listener(move |_, _: &Close, _, cx| {
                cx.emit(FileSurfaceEvent::Close(owner));
            }))
            .on_action(
                cx.listener(move |view, _: &ferrite_pane::find::Next, _, cx| {
                    view.step_find(owner, true, cx)
                }),
            )
            .on_action(
                cx.listener(move |view, _: &ferrite_pane::find::Previous, _, cx| {
                    view.step_find(owner, false, cx)
                }),
            )
            .on_action(
                cx.listener(move |view, _: &ferrite_pane::find::Close, window, cx| {
                    view.close_find(owner, window, cx)
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    view.pressed = Some(owner);
                    cx.emit(FileSurfaceEvent::Pressed(owner));
                }),
            )
            .into_any_element()
    }

    /// ⌘F in `owner`'s preview: the bar over it (or the keyboard back to
    /// it). False when the preview shows nothing to search.
    pub(in crate::cockpit) fn open_find(
        &mut self,
        owner: PaneIdentity,
        preview: &Preview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(document) = preview
            .document()
            .filter(|document| document.text().is_some())
        else {
            return false;
        };
        let tab = self.tab(owner, preview);
        if tab
            .find
            .as_ref()
            .is_none_or(|find| find.path != document.path)
        {
            tab.find = Some(PreviewFind::new(
                ferrite_pane::find::Find::new(cx, move |view: &mut Self, cx| {
                    view.refind(owner, cx)
                }),
                document.path,
            ));
            cx.notify();
        }
        if let Some(find) = &tab.find {
            find.bar.focus(window, cx);
        }
        true
    }

    /// Search the preview's document again (its line changed): a new query
    /// starts on its first occurrence in view, or the first below it.
    fn refind(&mut self, owner: PaneIdentity, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(&owner) else {
            return;
        };
        let Some(document) = tab.preview.document() else {
            return;
        };
        let Some(text) = document.text() else {
            return;
        };
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
            reveal_found(tab, text, rich.is_some(), true);
        }
        cx.notify();
    }

    /// ⏎ / ⇧⏎ in the preview's bar.
    fn step_find(&mut self, owner: PaneIdentity, forward: bool, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(&owner) else {
            return;
        };
        let Some(document) = tab.preview.document() else {
            return;
        };
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
    fn close_find(&mut self, owner: PaneIdentity, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(&owner) {
            tab.find = None;
            window.blur(cx);
            cx.notify();
        }
    }

    /// `try again` on a file that could not be read: it is read afresh.
    fn retry(&mut self, owner: PaneIdentity, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(&owner) {
            tab.documents.remove(path);
            tab.preview.open(path.to_path_buf(), None, &[], window, cx);
        }
        cx.notify();
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn tab_for_test(&self, owner: PaneIdentity) -> &PreviewTab {
        &self.tabs[&owner]
    }
}

/// What the head reads for `document`: a command's name, else the path
/// from the Thread's workspace, else from `~`: the tab above already reads
/// its name.
fn preview_title(workspace: Option<&Path>, document: &Document) -> String {
    if let Some(name) = &document.name {
        return name.clone();
    }
    workspace
        .and_then(|root| {
            document
                .path
                .strip_prefix(root)
                .ok()
                .map(|relative| relative.display().to_string())
        })
        .unwrap_or_else(|| home_relative(&document.path))
}

/// A page head's title: its name and a ▾, a press opening its menu
/// (`MenuTarget::Page`) under it.
fn page_title_menu(owner: PaneIdentity, title: String, cx: &mut Context<FileSurfaceView>) -> Div {
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
            .on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                cx.stop_propagation();
                // Under the title, its left edge on the title's.
                let at = drawn.get().map_or(event.position(), |title| {
                    gpui::point(title.left(), title.bottom() + px(MENU_UNDER_GAP))
                });
                cx.emit(FileSurfaceEvent::PageMenu(owner, at));
            })),
    )
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
        if let Some(thread) = self.focused_thread(cx).filter(|thread| self.uses_remote_files(*thread, cx)) {
            self.open_in_files(thread, path, line, true, cx);
            return;
        }
        if self.on_machine(cx) {
            self.nav_notice.hint("Start a Thread in this remote workspace before opening its files.");
            cx.notify();
            return;
        }
        if let Some(pane) = self.panes.get(self.focused(cx)) {
            pane.preview.open(path, line, &[], window, cx);
        }
        cx.notify();
    }

    /// Whether Pane `index`'s Panel shows its preview: a File or the
    /// Browser is its shown Surface — always, for a draft, which has no
    /// other.
    pub(in crate::cockpit) fn preview_shown(&self, index: usize, cx: &gpui::App) -> bool {
        let Some(pane) = self.panes.get(index) else {
            return false;
        };
        pane.preview.is_open()
            && match pane.thread() {
                None => true,
                Some(_) => matches!(
                    self.shown_surface(index, cx),
                    Some(SurfaceId::File(_) | SurfaceId::Browser)
                ),
            }
    }

    /// Close Pane `index`'s preview (escape in it, ⌘W while it holds focus):
    /// its tab closes and the keyboard is the Pane's — or, a draft's Panel
    /// shown for the preview alone, it hides.
    pub(in crate::cockpit) fn close_preview(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(pane) = self.panes.get(index) else {
            return;
        };
        match (pane.thread(), pane.preview.document()) {
            (Some(thread), Some(document)) => {
                let id = document_surface(&document);
                self.close_surface(self.cockpit.read(cx).thread_key(thread), &id, cx);
            }
            _ => pane.preview.close(),
        }
        self.focus_pane(index, cx);
        cx.notify();
    }

    /// A File (or the Browser) tab of `thread` closed: the preview puts its
    /// document away if it shows it, and forgets it.
    pub(in crate::cockpit) fn close_preview_of(&mut self, thread: ThreadId, path: &Path, cx: &mut Context<Self>) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let pane = &self.panes[index];
        let owner = pane.identity;
        self.file_surface
            .update(cx, |files, _| files.forget_document(owner, path));
        if pane.preview.document().is_some_and(|document| document.path == path) {
            pane.preview.close();
        }
    }

    /// A File tab's title: the command's or page's own name where it has
    /// one, else the file's.
    pub(in crate::cockpit) fn file_surface_title(
        &self,
        thread: ThreadId,
        path: &Path,
        cx: &gpui::App,
    ) -> SharedString {
        let named = self
            .file_surface
            .read(cx)
            .document(PaneIdentity::Thread(thread), path)
            .and_then(|document| document.name.clone());
        named
            .unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().into_owned())
            .into()
    }

    /// Each frame, before the board is measured: a remote file asked for
    /// opens in Files, a Browser annotation's capture is read, and the File
    /// Surface tends every preview (`FileSurfaceView::tend`); a document
    /// that opened goes through the Panel's door as its Surface, and the
    /// preview follows the File tab the Panel shows.
    pub(in crate::cockpit) fn tend_previews(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for pane in &self.panes {
            pane.preview.settle_attachment_open(window, cx);
        }
        let requests: Vec<_> = self.panes.iter().flat_map(|pane| {
            pane.preview.take_file_requests().into_iter().map(|request| (pane.thread(), request))
        }).collect();
        for (thread, request) in requests {
            match thread {
                Some(thread) => self.open_in_files(thread, request.path, request.line, true, cx),
                None => self.nav_notice.hint("Start a Thread in this remote workspace before opening its files."),
            }
        }
        for pane in &self.panes {
            for annotation in pane.preview.take_annotation_requests() {
                if annotation.original().is_none() {
                    pane.preview.loading_annotation(annotation, window);
                    continue;
                }
                let Some(thread) = pane.thread() else {
                    pane.preview.annotation_error(
                        &annotation,
                        "Open the owning Thread to review this browser capture".into(),
                        window,
                    );
                    continue;
                };
                let image = self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.browser_annotation_image(thread, annotation.clone()));
                pane.preview.loading_annotation(annotation.clone(), window);
                self.file_surface.update(cx, |files, _| {
                    files.wait_annotation(pane.identity, &pane.preview, annotation, image)
                });
            }
        }
        let panes: Vec<_> = self
            .panes
            .iter()
            .map(|pane| (pane.identity, pane.preview.clone()))
            .collect();
        let tended = self
            .file_surface
            .update(cx, |files, _| files.tend(&panes, window));
        if tended.waiting {
            // The image is read on a thread of its own, and only a frame
            // looks for it: ask for another, or an idle window never shows it.
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(crate::cockpit::beside::ANSWER_POLL).await;
                this.update(cx, |_, cx| cx.notify()).ok();
            })
            .detach();
        }
        for (thread, id) in tended.opened {
            let thread = self.cockpit.read(cx).thread_key(thread);
            if self.active_surface(thread).as_ref() != Some(&id) || !self.panel_open(thread) {
                self.apply_surface(thread, SurfaceAction::Open(id));
            }
        }
        // After the opened ones take their tabs, or the tab shown before
        // would read its own document back over the one just opened.
        self.sync_previews(window, cx);
    }

    /// Keep each Thread's preview showing the document its Panel shows: a
    /// File tab pressed brings its document back, as last read, or read
    /// afresh after a restart.
    fn sync_previews(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut unfillable = Vec::new();
        for index in 0..self.panes.len() {
            let Some(thread) = self.panes[index].thread() else {
                continue;
            };
            let wanted = match self.active_surface(self.cockpit.read(cx).thread_key(thread)) {
                Some(SurfaceId::File(path)) => path,
                Some(SurfaceId::Browser) => PathBuf::from(BROWSER_PATH),
                _ => continue,
            };
            let pane = &self.panes[index];
            if pane.preview.document().is_some_and(|document| document.path == wanted) {
                continue;
            }
            let kept = self.file_surface.read(cx).document(pane.identity, &wanted).cloned();
            match kept {
                Some(document) => pane.preview.restore(document),
                None if wanted == Path::new(BROWSER_PATH) => pane.preview.show_browser(),
                None => match self.sent_annotation(thread, &wanted, cx) {
                    Some(annotation) => pane.preview.review_annotation(annotation, window),
                    // An image opens in the lightbox and never becomes the
                    // tab's document, so it would open again every frame.
                    // Only the history can claim it, and that streams in
                    // after the first frames draw the Thread.
                    None if ferrite_pane::image_viewer::is_image(&wanted) => {
                        if self.cockpit.read(cx).thread(thread).is_some_and(|open| open.history_loaded()) {
                            unfillable.push((thread, SurfaceId::File(wanted)));
                        }
                    }
                    None => pane.preview.open(wanted, None, &[], window, cx),
                },
            }
        }
        for (thread, id) in unfillable {
            self.forget_surface(self.cockpit.read(cx).thread_key(thread), &id);
        }
    }

    /// A Browser annotation's review tab is keyed by its capture's PNG,
    /// which opened as a file would go to the lightbox: after a restart the
    /// review is rebuilt from the prompt in history that sent it.
    fn sent_annotation(
        &self,
        thread: ThreadId,
        original: &Path,
        cx: &gpui::App,
    ) -> Option<ferrite_core::browser_annotations::Annotation> {
        self.cockpit.read(cx).thread(thread)?.transcript().blocks().iter().find_map(|block| {
            let ferrite_core::transcript::Body::Prompt(words) = &block.body else {
                return None;
            };
            ferrite_core::browser_annotations::split(words.clone())
                .2
                .into_iter()
                .find(|annotation| annotation.original() == Some(original))
        })
    }

    /// The File Surface's body for Pane `owner`, drawn by its view.
    pub(in crate::cockpit) fn panel_preview(
        &self,
        owner: PaneIdentity,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.file_surface_inputs(owner, cx) {
            Some(inputs) => self
                .file_surface
                .update(cx, |files, cx| files.card(inputs, cx)),
            None => div().into_any_element(),
        }
    }

    /// What the card draws of the window for Pane `owner`: its preview,
    /// the Thread's checkout, and the live browser's body while that is
    /// the document.
    fn file_surface_inputs(
        &self,
        owner: PaneIdentity,
        cx: &mut Context<Self>,
    ) -> Option<FileSurfaceInputs> {
        let pane = &self.panes[self.index_of(owner)?];
        let browser = pane
            .preview
            .document()
            .is_some_and(|document| matches!(document.body, Body::Browser))
            .then(|| self.browser_body(pane.thread().map(|thread| self.cockpit.read(cx).thread_key(thread)), cx));
        Some(FileSurfaceInputs {
            owner,
            preview: pane.preview.clone(),
            workspace: pane.thread().and_then(|thread| self.thread_path(thread, cx)),
            browser,
        })
    }

    /// What the card asked: close it, the keyboard to its Pane, a page's
    /// menu.
    pub(in crate::cockpit) fn on_file_surface_event(
        &mut self,
        _: &Entity<FileSurfaceView>,
        event: &FileSurfaceEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match *event {
            FileSurfaceEvent::Close(owner) => {
                if let Some(index) = self.index_of(owner) {
                    self.close_preview(index, cx);
                }
            }
            FileSurfaceEvent::Pressed(owner) => {
                if let Some(index) = self.index_of(owner) {
                    self.focus_pane(index, cx);
                    cx.notify();
                }
            }
            FileSurfaceEvent::PageMenu(owner, at) => self.open_context_menu_at(
                MenuTarget::Page(owner),
                at,
                gpui::Anchor::TopLeft,
                None,
                cx,
            ),
        }
    }

    /// The page Pane `owner`'s preview shows, if it shows one.
    pub(in crate::cockpit) fn page_source(&self, owner: PaneIdentity) -> Option<PageSource> {
        let pane = self.panes.get(self.index_of(owner)?)?;
        Some(pane.preview.document()?.page()?.source().clone())
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

    /// What the File Surface keeps of Pane `index`'s preview.
    #[cfg(test)]
    pub(crate) fn preview_tab<'a>(&self, index: usize, cx: &'a gpui::App) -> &'a PreviewTab {
        self.file_surface.read(cx).tab_for_test(self.panes[index].identity)
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
fn unshown(
    why: &Unshown,
    path: &Path,
    owner: PaneIdentity,
    cx: &mut Context<FileSurfaceView>,
) -> AnyElement {
    let (note, detail) = match why {
        Unshown::Binary => ("This is not a text file", None),
        Unshown::Unreadable(error) => ("Could not read this file", Some(error.clone())),
    };
    let original = path.to_path_buf();
    let mut doors: Vec<Door> = vec![(
        "open original",
        Box::new(move |window, cx| ferrite_pane::preview::open_original(&original, window, cx)),
    )];
    if let (Unshown::Unreadable(_), Some(_)) = (why, owner.thread()) {
        let view = cx.entity();
        let path = path.to_path_buf();
        doors.insert(
            0,
            (
                "try again",
                Box::new(move |window, cx| {
                    view.update(cx, |view, cx| view.retry(owner, &path, window, cx))
                }),
            ),
        );
    }
    cant_show(note, detail, doors, cx)
}

type Door = (&'static str, Box<dyn Fn(&mut Window, &mut gpui::App)>);

fn cant_show(
    note: &'static str,
    detail: Option<SharedString>,
    doors: Vec<Door>,
    cx: &mut Context<FileSurfaceView>,
) -> AnyElement {
    use ferrite_ui::theme::*;
    let doors = doors.into_iter().map(|(label, open)| {
        let id = SharedString::from(format!("preview-{}", label.replace(' ', "-")));
        let selector = id.clone();
        ferrite_ui::components::form_button(id, cx)
            .debug_selector(move || selector.to_string())
            .h(px(ROW))
            .px(px(CH))
            .child(ferrite_ui::components::form_label(label, INK_SUBTLE))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                open(window, cx);
            })
    });
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
        .child(div().flex().gap(px(SPACE_2)).children(doors))
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
        find.reveal_when_painted(start_in_view);
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
pub(in crate::cockpit) fn settle_markdown_found(
    find: &mut PreviewFind,
    scroll: &ScrollHandle,
    window: &mut Window,
) {
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

/// The Surface a document is: the live browser, or a File by its path.
pub(in crate::cockpit) fn document_surface(document: &Document) -> SurfaceId {
    match document.body {
        Body::Browser => SurfaceId::Browser,
        _ => SurfaceId::File(document.path.clone()),
    }
}
