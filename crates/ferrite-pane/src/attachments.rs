//! The files a draft carries, over the Composer (SPEC §5.5): an image as a
//! thumbnail card (Claude's), any other file as a shelf chip —
//! `SHELF_CHIP_H` high at `R_CONTROL` on `paint::RAISED`, Geist `FS_META`,
//! the file mark and the name in `LINK_INK`. The card is the sent image's
//! too (`thumbnail`). This module owns presentation and
//! opening (in the Pane's preview, the Panel's Preview tab: `Preview::open`); callers
//! only supply paths and, for a draft, a removal callback. The prompt codec owns persistence. A draft's review comments
//! (ADR 0021) ride the same shelf as one more chip. Its replies (a
//! selection's Reply, `crate::quote`) are cards beside the images
//! (`reply_card`); a sent prompt draws its quotes as Quote rows over its
//! bubble (`quote_row`).

use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use gpui::{prelude::*, px, App, ElementId, IntoElement, SharedString, Window};

use crate::preview::Preview;
use crate::quote::Reply;

/// Nothing to install now: delivered files draw in the float grammar rather
/// than on the kit's stock attachment cards, so the kit's own tokens are no
/// longer captured. Kept for the theme's init order (`init_components`).
pub fn init(_cx: &mut App) {}

type Remove = Rc<dyn Fn(&PathBuf, &mut Window, &mut App)>;
type DropReview = Rc<dyn Fn(&mut Window, &mut App)>;
type RemoveReply = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Attachments {
    id: ElementId,
    files: Vec<PathBuf>,
    preview: Preview,
    on_remove: Option<Remove>,
    review: usize,
    on_drop_review: Option<DropReview>,
    replies: Vec<Reply>,
    on_remove_reply: Option<RemoveReply>,
    island: bool,
}

impl Attachments {
    pub fn new(id: impl Into<ElementId>, files: Vec<PathBuf>, preview: &Preview) -> Self {
        Self {
            id: id.into(),
            files,
            preview: preview.clone(),
            on_remove: None,
            review: 0,
            on_drop_review: None,
            replies: Vec::new(),
            on_remove_reply: None,
            island: false,
        }
    }

    /// Pending attachments as chips on the shelf above the prompt.
    pub fn in_island(mut self) -> Self {
        self.island = true;
        self
    }

    pub fn on_remove(
        mut self,
        callback: impl Fn(&PathBuf, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_remove = Some(Rc::new(callback));
        self
    }

    /// `comments` review comments wait with the files: one chip for all.
    pub fn review(mut self, comments: usize) -> Self {
        self.review = comments;
        self
    }

    /// The replies the draft carries, as cards beside the images.
    pub fn replies(mut self, replies: Vec<Reply>) -> Self {
        self.replies = replies;
        self
    }

    /// A Reply card's ×: that reply (by index) goes.
    pub fn on_remove_reply(
        mut self,
        callback: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_remove_reply = Some(Rc::new(callback));
        self
    }

    /// The review chip's ×: every gathered comment goes.
    pub fn on_drop_review(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_drop_review = Some(Rc::new(callback));
        self
    }
}

/// The draft's files: each image as a thumbnail card (`image_card`) in a
/// wrapping row, then the other files as shelf chips, wrapping
/// (`SHELF_CHIP_H` on `paint::RAISED`) — the `FILE` mark, the name in
/// `LINK_INK` cut at `ATTACH_CHIP_MAX_W` (a file name is machine text),
/// and, on a draft, a quiet `×` that removes it (a button). The buttons
/// sit in the `PromptAttachment` key context, so their Enter never sends.
/// A click on a card or a chip opens the image in the lightbox, among the
/// tray's images, or the file in the Pane's preview.
fn chips(attachments: Attachments) -> gpui::AnyElement {
    use ferrite_ui::pointer::Pointer as _;
    use ferrite_ui::theme;
    use gpui::{div, rgb, SharedString};
    let Attachments {
        id,
        files,
        preview,
        on_remove,
        review,
        on_drop_review,
        replies,
        on_remove_reply,
        island,
    } = attachments;
    let (images, others): (Vec<_>, Vec<_>) = files
        .into_iter()
        .enumerate()
        .partition(|(_, path)| crate::image_viewer::is_image(path));
    let gallery: Arc<[PathBuf]> = images.iter().map(|(_, path)| path.clone()).collect();
    let cards: Vec<_> = images
        .iter()
        .map(|(index, path)| image_card(*index, path, &gallery, &preview, on_remove.clone()))
        .collect();
    let others_empty = others.is_empty();
    let chips = others.into_iter().map(|(index, path)| {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let host = preview.clone();
        let open = path.clone();
        div()
            .id(("attachment", index))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::CH))
            .h(px(theme::ATTACH_CHIP_H))
            .max_w(px(theme::ATTACH_CHIP_MAX_W))
            .min_w_0()
            .px(px(theme::SPACE_2))
            .rounded(px(theme::R_CONTROL))
            .bg(theme::paint::RAISED)
            .cursor_pointer()
            .hover_carried(format!("attachment-{index}-{}", open.display()))
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_META))
            .line_height(px(theme::LH_META))
            .font_weight(theme::W_BODY)
            .text_color(rgb(theme::LINK_INK))
            .whitespace_nowrap()
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(px(theme::ATTACH_THUMB))
                    .child(ferrite_ui::icons::icon(
                        ferrite_ui::icons::FILE,
                        theme::ATTACH_THUMB,
                        theme::LINK_INK,
                    )),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(name.clone())),
            )
            .when_some(on_remove.clone(), |chip, remove| {
                let removed = path.clone();
                chip.child(
                    remove_button(("remove-attachment", index))
                        .accessibility_label(format!("Remove {name}"))
                        .tooltip_builder(ferrite_ui::menu::tooltip(format!(
                            "Remove {}",
                            path.display()
                        )))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            remove(&removed, window, cx);
                        }),
                )
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                host.open(open.clone(), None, &[], window, cx);
            })
    });
    let review = (review > 0).then(|| review_chip(review, on_drop_review));
    let replies: Vec<_> = replies
        .into_iter()
        .enumerate()
        .map(|(index, reply)| reply_card(index, reply, on_remove_reply.clone()))
        .collect();
    let has_chips = !others_empty || review.is_some();
    let chips = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(theme::CH))
        .min_w_0()
        .max_w_full()
        .children(chips)
        .children(review);
    let has_cards = !cards.is_empty() || !replies.is_empty();
    let cards = div()
        .flex()
        .flex_wrap()
        .gap(px(theme::SPACE_2))
        .min_w_0()
        .max_w_full()
        .children(replies)
        .children(cards);
    div()
        .id(id)
        .when(island, |shelf| {
            shelf.debug_selector(|| "attachment-island-content".into())
        })
        .flex()
        .flex_col()
        .gap(px(theme::CH))
        .min_w_0()
        .max_w_full()
        .when(has_cards, |shelf| shelf.child(cards))
        .when(has_chips, |shelf| shelf.child(chips))
        .into_any_element()
}

/// An image waiting in the tray as Claude shows one: the picture covering
/// a `COMPOSER_THUMB` card (`thumbnail`), a button — a tab stop,
/// Enter/Space — that opens it in the lightbox among the tray's images;
/// on a draft, its `×` in the corner — shown under the pointer or on
/// keyboard focus — removes it.
fn image_card(
    index: usize,
    path: &Path,
    gallery: &Arc<[PathBuf]>,
    preview: &Preview,
    on_remove: Option<Remove>,
) -> gpui::AnyElement {
    use ferrite_ui::theme;
    use gpui::div;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let group = SharedString::from(format!("tray-image-{index}"));
    let host = preview.clone();
    let open = path.to_path_buf();
    let gallery = gallery.clone();
    let card = ferrite_ui::components::button(("preview-attachment", index))
        .tab_stop(true)
        .p_0()
        .size(px(theme::COMPOSER_THUMB))
        .rounded(px(theme::R_CARD))
        .key_context("PromptAttachment")
        .accessibility_label(format!("Preview {name}"))
        .child(thumbnail(
            ("tray-image", index),
            path,
            (theme::COMPOSER_THUMB, theme::COMPOSER_THUMB),
        ))
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            host.open(open.clone(), None, &gallery, window, cx);
        });
    let remove = on_remove.map(|remove| {
        let removed = path.to_path_buf();
        tile_remove(
            format!("tray-image-{index}-remove"),
            ("remove-attachment", index),
            group.clone(),
            format!("Remove {name}"),
            format!("Remove {}", path.display()),
            move |window, cx| remove(&removed, window, cx),
        )
    });
    div()
        .id(("tray-image-card", index))
        .debug_selector(move || format!("tray-image-{index}"))
        .group(group)
        .relative()
        .flex_shrink_0()
        .child(card)
        .children(remove)
        .into_any_element()
}

/// An image as a card, in the tray and over a sent prompt (Claude's): the
/// picture covering `width` × `height` at `R_CARD` in a 1px `LINE` ring,
/// its caption (`name · W×H · 41 KB`, `pane::image_caption`) its tooltip.
/// The caller wires what a click does.
pub(crate) fn thumbnail(
    id: impl Into<ElementId>,
    path: &Path,
    (width, height): (f32, f32),
) -> gpui::Stateful<gpui::Div> {
    use ferrite_ui::theme;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let caption = crate::pane::image_caption(&name, crate::pane::image_facts(path));
    gpui::div()
        .id(id)
        .flex_shrink_0()
        .rounded(px(theme::R_CARD))
        .border_1()
        .border_color(theme::paint::LINE)
        .overflow_hidden()
        .tooltip(ferrite_ui::menu::tooltip(caption))
        .child(
            gpui::img(path.to_path_buf())
                .w(px(width))
                .h(px(height))
                .rounded(px(theme::R_CARD - theme::HAIRLINE_W))
                .object_fit(gpui::ObjectFit::Cover),
        )
}

/// A quoted passage as Claude draws one over the sent prompt's bubble: the
/// word Quote in `INK_SUBTLE`, then the passage cut at `QUOTE_ROW_LINES`
/// lines, behind a `QUOTE_RULE_W` `LINE` rule (Markdown's quote rule), and
/// under it the words the operator replied with, when there are any.
pub fn quote_row(index: usize, reply: Reply) -> gpui::Stateful<gpui::Div> {
    use ferrite_ui::theme;
    use gpui::{div, rgb, SharedString};
    div()
        .id(("quote", index))
        .debug_selector(move || format!("quote-row-{index}"))
        .flex()
        .flex_col()
        .min_w_0()
        .border_l(px(theme::QUOTE_RULE_W))
        .border_color(theme::paint::LINE)
        .pl(px(theme::SPACE_2))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .font_weight(theme::W_BODY)
        .child(
            div()
                .text_color(rgb(theme::INK_SUBTLE))
                .child(SharedString::from("Quote")),
        )
        .child(
            div()
                .text_color(rgb(theme::INK_STRONG))
                .line_clamp(theme::QUOTE_ROW_LINES)
                .text_ellipsis()
                .child(SharedString::from(reply.quote)),
        )
        .when(!reply.text.is_empty(), |row| {
            row.child(
                div()
                    .debug_selector(move || format!("quote-row-{index}-words"))
                    .text_color(rgb(theme::INK_STRONG))
                    .line_clamp(theme::QUOTE_ROW_LINES)
                    .text_ellipsis()
                    .child(SharedString::from(reply.text)),
            )
        })
}

/// A reply waiting in the tray (`claude/reply-sent-chip.png`), one of the
/// tray's tiles: an image card's own `COMPOSER_THUMB` square at `R_CARD` in
/// its `LINE` ring and `×` (`tile_remove`), on the file chip's `RAISED`
/// ground. Its top is a quote bar holding what the operator typed (the
/// passage itself when they typed nothing), cut at `REPLY_TILE_LINES`; its
/// foot the muted `Reply` label, where a file's type would stand.
fn reply_card(index: usize, reply: Reply, on_remove: Option<RemoveReply>) -> gpui::AnyElement {
    use ferrite_ui::theme;
    use gpui::{div, rgb};
    let group = SharedString::from(format!("tray-reply-{index}"));
    let words = if reply.text.is_empty() {
        reply.quote
    } else {
        reply.text
    };
    let remove = on_remove.map(|remove| {
        tile_remove(
            format!("tray-reply-{index}-remove"),
            ("remove-reply", index),
            group.clone(),
            "Remove reply".into(),
            "Remove reply".into(),
            move |window, cx| remove(index, window, cx),
        )
    });
    div()
        .id(("tray-reply-card", index))
        .debug_selector(move || format!("tray-reply-{index}"))
        .group(group)
        .relative()
        .flex()
        .flex_col()
        .justify_between()
        .flex_shrink_0()
        .size(px(theme::COMPOSER_THUMB))
        .p(px(theme::SPACE_1_5))
        .rounded(px(theme::R_CARD))
        .bg(theme::paint::RAISED)
        .border_1()
        .border_color(theme::paint::LINE)
        .overflow_hidden()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .font_weight(theme::W_BODY)
        .text_color(rgb(theme::INK_SUBTLE))
        .child(
            div()
                .flex()
                .gap(px(theme::SPACE_1_5))
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(theme::QUOTE_RULE_W))
                        .h(px(theme::LH_META))
                        .bg(theme::paint::LINE),
                )
                .child(
                    div()
                        .debug_selector(move || format!("tray-reply-{index}-words"))
                        .min_w_0()
                        .line_clamp(theme::REPLY_TILE_LINES)
                        .text_ellipsis()
                        .child(SharedString::from(words)),
                ),
        )
        .child(
            div()
                .debug_selector(move || format!("tray-reply-{index}-badge"))
                .child("Reply"),
        )
        .children(remove)
        .into_any_element()
}

/// A tile's `×` in its top corner (an image card's, a reply's): clear until
/// the pointer is over the tile (`group`) or the keyboard reaches it — a
/// tab stop either way, in the `PromptAttachment` context.
fn tile_remove(
    selector: String,
    id: (&'static str, usize),
    group: SharedString,
    label: String,
    tooltip: String,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> gpui::Div {
    use ferrite_ui::theme;
    gpui::div()
        .debug_selector(move || selector.clone())
        .absolute()
        .top(px(theme::THUMB_REMOVE_INSET))
        .right(px(theme::THUMB_REMOVE_INSET))
        .child(
            ferrite_ui::components::button(id)
                .opacity(0.)
                .group_hover(group, |style| style.opacity(1.))
                .focus_visible(|style| ferrite_ui::components::control_focus(style).opacity(1.))
                .tab_stop(true)
                .p_0()
                .size(px(theme::THUMB_REMOVE))
                .rounded_full()
                .bg(theme::paint::FLOAT)
                .border_1()
                .border_color(theme::paint::LINE)
                .key_context("PromptAttachment")
                .accessibility_label(label)
                .tooltip_builder(ferrite_ui::menu::tooltip(tooltip))
                .child(ferrite_ui::icons::icon(
                    ferrite_ui::icons::CLOSE,
                    theme::ATTACH_REMOVE_ICON,
                    theme::INK_STRONG,
                ))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    on_click(window, cx);
                }),
        )
}

/// The draft's review comments as one `.att` chip: the pencil, `N review
/// comments` in `LINK_INK`, and a quiet `×` that drops them all.
fn review_chip(comments: usize, on_drop: Option<DropReview>) -> gpui::Div {
    use ferrite_ui::theme;
    use gpui::{div, rgb, SharedString};
    let label = format!(
        "{comments} review comment{}",
        if comments == 1 { "" } else { "s" }
    );
    div()
        .debug_selector(|| "review-attachment".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::CH))
        .h(px(theme::ATTACH_CHIP_H))
        .px(px(theme::SPACE_2))
        .rounded(px(theme::R_CONTROL))
        .bg(theme::paint::RAISED)
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .font_weight(theme::W_BODY)
        .text_color(rgb(theme::LINK_INK))
        .whitespace_nowrap()
        .child(ferrite_ui::icons::icon(
            ferrite_ui::icons::PENCIL,
            theme::ATTACH_THUMB,
            theme::LINK_INK,
        ))
        .child(SharedString::from(label))
        .when_some(on_drop, |chip, on_drop| {
            chip.child(
                remove_button("drop-review-comments")
                    .debug_selector(|| "drop-review-comments".into())
                    .accessibility_label("Remove the review comments")
                    .tooltip_builder(ferrite_ui::menu::tooltip("Remove the review comments"))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        on_drop(window, cx);
                    }),
            )
        })
}

/// A chip's remove control: the `close` icon (`ATTACH_REMOVE_ICON`) in
/// `INK_SUBTLE` in an `ATTACH_REMOVE`-wide slot, the kit ghost's fill under the
/// pointer — a tab stop in the `PromptAttachment` context, so its Enter
/// never sends.
fn remove_button(id: impl Into<ElementId>) -> gpui::component::button::Button {
    use ferrite_ui::theme;
    ferrite_ui::components::button(id)
        .tab_stop(true)
        .p_0()
        .flex_shrink_0()
        .h(px(theme::ATTACH_CHIP_H))
        .w(px(theme::ATTACH_REMOVE))
        .rounded(px(theme::R_TIGHT))
        .key_context("PromptAttachment")
        .child(ferrite_ui::icons::icon(
            ferrite_ui::icons::CLOSE,
            theme::ATTACH_REMOVE_ICON,
            theme::INK_SUBTLE,
        ))
}

impl RenderOnce for Attachments {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // The draft's shelf and the delivered prompt's files are one recipe;
        // only the draft can remove.
        chips(self)
    }
}

/// A file link in prose: the path as the prose wrote it on the inline-code
/// chip (`rich::code_chip`, as neither reference app colours a path), a
/// `:line` suffix in `INK_SUBTLE`. It is a path target
/// (`file_links::wire_inline`): an underline only under the pointer, ⌘-click to the Thread's preview; a plain
/// click opens it in the Pane's preview too, at its `:line`. The native Markdown flow
/// reserves the returned size and wraps the link atomically, so the width is
/// measured in the face and size it is drawn in (the prose's own): the path
/// and the location each shaped whole.
pub fn inline_file(
    file: crate::file_links::FileLink,
    label: &str,
    preview: Option<&Preview>,
    scope: Option<gpui::SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> (gpui::Size<gpui::Pixels>, gpui::AnyElement) {
    use ferrite_ui::theme;
    use gpui::component::button::ButtonVariants as _;
    use gpui::rgb;

    let base = file
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let location = file
        .location
        .as_ref()
        .map(|line| format!(":{line}"))
        .unwrap_or_default();
    // The path as the prose wrote it, its `:line` drawn apart.
    let label = label.trim();
    let label = label.strip_suffix(location.as_str()).unwrap_or(label);
    let name = if label.is_empty() {
        base.clone()
    } else {
        label.to_string()
    };
    let image = crate::image_viewer::is_image(&file.path);
    let size_px = window.text_style().font_size.to_pixels(window.rem_size());
    let link_w = inline_file_width(&name, &location, image, size_px, window);
    let line_h = window.line_height();
    // The line reserves the path's exact advance, as the prototype's inline
    // span takes it; the link's own box keeps its pixel to spare and hangs
    // past it, so the words after it stand where the browser sets them.
    // The chip's padding is room on the line, either side of the path.
    let pad = px(2.0 * theme::INLINE_CODE_PAD_X);
    let size = gpui::size(
        inline_file_advance(&name, &location, link_w, size_px, window) + pad,
        line_h,
    );
    let link_w = link_w + pad;
    let host = preview.cloned();
    let selector = format!("file-attachment-{}", file.path.display());
    let accessibility = format!("Open {name}");
    let mut target = crate::file_links::PathTarget::new(file.path.clone()).at_line(
        file.location
            .as_deref()
            .and_then(crate::file_links::location_line),
    );
    if let Some(scope) = scope {
        target = target.in_scope(scope);
    }
    let reader = target.clone();
    let open = move |event: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
        gpui::base::TextSelection::end(window, cx);
        cx.stop_propagation();
        if event.modifiers().secondary() {
            crate::file_links::open_target(&reader, window, cx);
            return;
        }
        match &host {
            Some(host) if file.path.exists() => {
                let line = file
                    .location
                    .as_deref()
                    .and_then(crate::file_links::location_line);
                host.open(file.path.clone(), line, &[], window, cx);
            }
            _ => file.open(window, cx),
        }
    };
    let link = crate::rich::code_chip(gpui::div())
        .id("inline-file-chip")
        .flex()
        .items_center()
        .w(link_w)
        .mt(px(theme::INLINE_CODE_INSET_Y))
        .h(line_h - px(2.0 * theme::INLINE_CODE_INSET_Y))
        .min_w_0()
        .not_italic()
        .child(
            // The path gives way to an ellipsis; the `:line` never does.
            gpui::div()
                .flex()
                .min_w_0()
                .child(gpui::div().min_w_0().truncate().child(name))
                .when(!location.is_empty(), |title| {
                    title.child(
                        gpui::div()
                            .flex_none()
                            .text_color(rgb(theme::INK_SUBTLE))
                            .child(location),
                    )
                }),
        );
    // The click and keyboard target lies over the link and draws nothing but
    // the focus ring.
    let clear: gpui::Hsla = gpui::transparent_black();
    let button = ferrite_ui::components::button("inline-file-action")
        .custom(ferrite_ui::pointer::button_variant(
            clear,
            rgb(theme::INLINE_CODE_INK).into(),
            clear,
            cx,
        ))
        .tab_stop(true)
        .key_context("PromptAttachment")
        .accessibility_label(accessibility)
        .absolute()
        .inset_0()
        .size_full()
        .min_w_0()
        .p_0()
        .rounded(px(theme::R_CONTROL))
        .on_click(open);
    let element = gpui::div()
        .id("inline-file")
        .debug_selector(move || selector.clone())
        .relative()
        .w(link_w)
        .h(size.height)
        .child(link)
        .child(button);
    (
        size,
        crate::file_links::wire_inline(element, target, f32::from(size_px), f32::from(line_h))
            .into_any_element(),
    )
}

/// What an inline file link reserves on its line: the name and `:line`
/// shaped in the code face at the prose's size, unrounded — the box's own
/// width (`link_w`) when the clamp decided it.
fn inline_file_advance(
    name: &str,
    location: &str,
    link_w: gpui::Pixels,
    size: gpui::Pixels,
    window: &mut Window,
) -> gpui::Pixels {
    use ferrite_ui::theme;
    let mut face = window.text_style();
    face.font_family = theme::FONT_CODE.into();
    face.font_weight = theme::W_BODY;
    face.font_style = gpui::FontStyle::Normal;
    let shaped = |text: &str| {
        if text.is_empty() {
            return px(0.);
        }
        window
            .text_system()
            .shape_line(
                gpui::SharedString::from(text.to_owned()),
                size,
                &[face.to_run(text.len())],
                None,
            )
            .width()
    };
    let exact = shaped(name) + shaped(location);
    if link_w <= px(theme::INLINE_FILE_MIN_W) || link_w >= px(theme::INLINE_FILE_MAX_W) {
        link_w
    } else {
        exact.min(link_w)
    }
}

/// An inline file link's width: the name and `:line` each shaped whole in
/// the code face at the prose's own size, rounded up to whole pixels with
/// 1px to spare, so a name that fits is never ellipsized. Clamped to
/// `INLINE_FILE_MIN_W`…`INLINE_FILE_MAX_W`.
pub(crate) fn inline_file_width(
    name: &str,
    location: &str,
    _image: bool,
    size: gpui::Pixels,
    window: &mut Window,
) -> gpui::Pixels {
    use ferrite_ui::theme;
    let mut face = window.text_style();
    face.font_family = theme::FONT_CODE.into();
    face.font_weight = theme::W_BODY;
    face.font_style = gpui::FontStyle::Normal;
    let width = |text: &str| {
        if text.is_empty() {
            return px(0.);
        }
        window
            .text_system()
            .shape_line(
                gpui::SharedString::from(text.to_owned()),
                size,
                &[face.to_run(text.len())],
                None,
            )
            .width()
            .ceil()
    };
    let text_w = width(name) + width(location) + px(1.);
    text_w.clamp(px(theme::INLINE_FILE_MIN_W), px(theme::INLINE_FILE_MAX_W))
}
