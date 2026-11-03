//! A finished turn's cards (Claude's): what it left to open
//! (`ferrite_core::transcript::outputs`), drawn under its answer
//! (`RowKind::Outputs`, theme `OUTPUT_*`).
//!
//! Each card, and each of its doors, is told apart by its place among
//! the turn's (`file-card-0`, `page-card-open-1`).
//!
//! - A **page card** (an `.html` file, a local address): its thumbnail,
//!   its title and what it is, `Open` — the page beside the transcript, in
//!   the Panel (`Preview::open_page`) — and `⋯`: open in the browser, copy
//!   its path or address.
//! - A **file card**: its mark, its name and where it lives, and `Open in ›`
//!   — Finder, or the app the system opens it with. A press on the card
//!   reads the file in the Panel (`Preview::open`).
//!
//! A path resolves against the Thread's checkout, as every path target
//! does (`FileLink::resolve`).

use std::path::PathBuf;

use ferrite_core::transcript::outputs::{Output, Page};
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, ClipboardItem, Context, MouseButton, Pixels, Point};

use super::{rows::TranscriptRow, TranscriptView};
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
    File(PathBuf),
}

/// One row of a card's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verb {
    Browser,
    Copy,
    Finder,
    DefaultApp,
}

impl Opened {
    fn verbs(&self) -> &'static [(&'static str, Verb)] {
        match self {
            Opened::Page(PageSource::File(_)) => {
                &[("Open in browser", Verb::Browser), ("Copy path", Verb::Copy)]
            }
            Opened::Page(PageSource::Address(_)) => {
                &[("Open in browser", Verb::Browser), ("Copy address", Verb::Copy)]
            }
            Opened::File(_) => &[("Finder", Verb::Finder), ("Default app", Verb::DefaultApp)],
        }
    }

    fn run(&self, verb: Verb, window: &mut gpui::Window, cx: &mut gpui::App) {
        match (self, verb) {
            (Opened::Page(page), Verb::Browser) => cx.open_url(&page.url()),
            (Opened::Page(PageSource::File(path)), Verb::Copy) | (Opened::File(path), Verb::Copy) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()))
            }
            (Opened::Page(PageSource::Address(address)), Verb::Copy) => {
                cx.write_to_clipboard(ClipboardItem::new_string(address.clone()))
            }
            (Opened::File(path), Verb::Finder) => cx.reveal_path(path),
            (Opened::File(path), Verb::DefaultApp) => FileLink {
                path: path.clone(),
                location: None,
            }
            .open(window, cx),
            _ => {}
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
            Output::File(path) => Opened::File(resolve(path)?),
        })
    }

    /// An outputs row: its cards, one under another.
    pub(super) fn render_outputs(
        &self,
        row: &TranscriptRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = format!("{}-{:?}", self.input.namespace, row.id());
        div()
            .debug_selector(|| "transcript-outputs".into())
            .flex()
            .flex_col()
            .gap(px(theme::OUTPUT_CARD_GAP))
            .children(
                row.outputs()
                    .iter()
                    .filter_map(|output| self.opened(output))
                    .enumerate()
                    .map(|(index, opened)| card(&key, index, opened, cx)),
            )
            .into_any_element()
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
                        cx.listener(move |view, _: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            view.output_menu = None;
                            target.run(verb, window, cx);
                            cx.notify();
                        }),
                    ),
            );
        }
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(open.at)
                    .snap_to_window_with_margin(px(theme::GRID_PAD))
                    .child(ferrite_ui::motion::menu_in(
                        "output-menu-in",
                        shell,
                        ferrite_ui::motion::Opens::Down,
                    )),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

/// One card. A press on its body opens it; its doors keep their presses.
fn card(row: &str, index: usize, opened: Opened, cx: &mut Context<TranscriptView>) -> AnyElement {
    let key = format!("{row}-{index}");
    let key = key.as_str();
    let (selector, lead, title, detail) = match &opened {
        Opened::Page(page) => ("page-card", thumbnail(), page.title(), page.kind().to_owned()),
        Opened::File(path) => (
            "file-card",
            file_mark(),
            path.file_name()
                .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned()),
            ferrite_ui::components::home_relative(path.parent().unwrap_or(path)),
        ),
    };
    let doors: Vec<AnyElement> = match &opened {
        Opened::Page(page) => {
            let shown = page.clone();
            vec![
                text_door(&format!("{key}-open"), format!("page-card-open-{index}"), "Open", false)
                    .on_click(cx.listener(move |view, _: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        view.input.preview.open_page(shown.clone(), window);
                    }))
                    .into_any_element(),
                more_door(&format!("{key}-more"), index, opened.clone(), cx),
            ]
        }
        Opened::File(_) => vec![open_in_door(&format!("{key}-open-in"), index, opened.clone(), cx)],
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
        .on_click(cx.listener(move |view, _: &gpui::ClickEvent, window, cx| match &body {
            Opened::Page(page) => view.input.preview.open_page(page.clone(), window),
            Opened::File(path) => view.input.preview.open(path.clone(), None, &[], window, cx),
        }))
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
        .child(icon(icons::GLOBE, theme::ICON_BUTTON_GLYPH, theme::INK_SUBTLE))
        .into_any_element()
}

/// A file's mark in its tile.
fn file_mark() -> AnyElement {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(theme::OUTPUT_ICON_TILE))
        .rounded(px(theme::R_CONTROL))
        .bg(paint::CARD)
        .child(icon(icons::FILE, theme::ICON_BUTTON_GLYPH, theme::INK_SUBTLE))
        .into_any_element()
}

/// A word door at a card's right (`Open`, `Open in ›`), lit under the
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
            door.child(icon(icons::CHEVRON_RIGHT, theme::FS_META, theme::INK_SUBTLE))
        })
}

/// A file card's `Open in ›`: its menu hangs from the press.
fn open_in_door(
    id: &str,
    index: usize,
    opened: Opened,
    cx: &mut Context<TranscriptView>,
) -> AnyElement {
    text_door(id, format!("file-card-open-in-{index}"), "Open in", true)
        .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
            cx.stop_propagation();
            view.open_output_menu(opened.clone(), event.position(), cx);
        }))
        .into_any_element()
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
        .child(icon(icons::ELLIPSIS, theme::ICON_BUTTON_GLYPH, theme::INK_SUBTLE))
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
            labels(Opened::File(PathBuf::from("/w/notes.md"))),
            ["Finder", "Default app"]
        );
        assert_eq!(
            labels(Opened::Page(PageSource::Address("http://localhost:3000".into()))),
            ["Open in browser", "Copy address"]
        );
        assert_eq!(
            labels(Opened::Page(PageSource::File(PathBuf::from("/w/a.html")))),
            ["Open in browser", "Copy path"]
        );
    }
}
