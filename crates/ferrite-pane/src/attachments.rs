//! The files a draft carries, as shelf chips over the Composer (SPEC §5.5):
//! `SHELF_CHIP_H` high at `R_CHIP` on `paint::RAISED`, Geist `FS_META`,
//! the image mark (or the file mark) and the name in `LINK_INK`. This module owns presentation and
//! opening (in the Pane's preview, the Panel's Preview tab: `Preview::open`); callers
//! only supply paths and, for a draft, a removal callback. The prompt codec owns persistence. A draft's review comments
//! (ADR 0021) ride the same shelf as one more chip.

use std::{path::PathBuf, rc::Rc};

use gpui::{prelude::*, px, App, ElementId, IntoElement, Window};

use crate::preview::Preview;

/// Nothing to install now: delivered files draw in the float grammar rather
/// than on the kit's stock attachment cards, so the kit's own tokens are no
/// longer captured. Kept for the theme's init order (`init_components`).
pub fn init(_cx: &mut App) {}

type Remove = Rc<dyn Fn(&PathBuf, &mut Window, &mut App)>;
type DropReview = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Attachments {
    id: ElementId,
    files: Vec<PathBuf>,
    preview: Preview,
    on_remove: Option<Remove>,
    review: usize,
    on_drop_review: Option<DropReview>,
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

    /// The review chip's ×: every gathered comment goes.
    pub fn on_drop_review(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_drop_review = Some(Rc::new(callback));
        self
    }
}

/// The files as shelf chips, wrapping (`SHELF_CHIP_H` on `paint::RAISED`),
/// a mark — an image's is a real button (a tab stop,
/// Enter/Space) that opens it in the Pane's preview, among the shelf's
/// images; any other file's the `FILE` mark (the chip opens it there too) —
/// then the name in `LINK_INK` cut at `ATTACH_CHIP_MAX_W` (a file name is
/// machine text), and, on a draft, a quiet `×` that removes it (a button
/// too). The buttons sit in the `PromptAttachment` key context, so their
/// Enter never sends. A click anywhere else on a chip opens the image
/// or the file.
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
        island,
    } = attachments;
    let gallery: Vec<PathBuf> = files
        .iter()
        .filter(|path| crate::image_viewer::is_image(path))
        .cloned()
        .collect();
    let gallery: Rc<[PathBuf]> = gallery.into();
    let chips = files.into_iter().enumerate().map(|(index, path)| {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let image = crate::image_viewer::is_image(&path);
        let mark = if image {
            let host = preview.clone();
            let open = path.clone();
            let gallery = gallery.clone();
            ferrite_ui::components::button(("preview-attachment", index))
                .tab_stop(true)
                .p_0()
                .size(px(theme::ATTACH_THUMB))
                .rounded(px(theme::R_TIGHT))
                .key_context("PromptAttachment")
                .accessibility_label(format!("Preview {name}"))
                .tooltip("Preview image")
                .child(ferrite_ui::icons::icon(
                    ferrite_ui::icons::IMAGE,
                    theme::ATTACH_THUMB,
                    theme::LINK_INK,
                ))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    host.open(open.clone(), None, &gallery, window, cx);
                })
                .into_any_element()
        } else {
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
                ))
                .into_any_element()
        };
        let host = preview.clone();
        let open = path.clone();
        let gallery = gallery.clone();
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
            .rounded(px(theme::R_CHIP))
            .bg(theme::paint::RAISED)
            .cursor_pointer()
            .hover_carried(format!("attachment-{index}-{}", open.display()))
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_META))
            .line_height(px(theme::LH_META))
            .font_weight(theme::W_BODY)
            .text_color(rgb(theme::LINK_INK))
            .whitespace_nowrap()
            .child(mark)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(name.clone())),
            )
            .when_some(on_remove.clone(), |chip, remove| {
                let removed = path.clone();
                chip.child(
                    ferrite_ui::components::button(("remove-attachment", index))
                        .tab_stop(true)
                        .p_0()
                        .flex_shrink_0()
                        .h(px(theme::ATTACH_CHIP_H))
                        .px(px(theme::SPACE_0_5))
                        .rounded(px(theme::R_TIGHT))
                        .key_context("PromptAttachment")
                        .accessibility_label(format!("Remove {name}"))
                        .tooltip(format!("Remove {}", path.display()))
                        .child(div().text_color(rgb(theme::TEXT_DIM)).child("\u{d7}"))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            remove(&removed, window, cx);
                        }),
                )
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                host.open(open.clone(), None, &gallery, window, cx);
            })
    });
    let review = (review > 0).then(|| review_chip(review, on_drop_review));
    div()
        .id(id)
        .when(island, |shelf| {
            shelf.debug_selector(|| "attachment-island-content".into())
        })
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(theme::CH))
        .min_w_0()
        .max_w_full()
        .children(chips)
        .children(review)
        .into_any_element()
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
        .rounded(px(theme::R_CHIP))
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
                ferrite_ui::components::button("drop-review-comments")
                    .debug_selector(|| "drop-review-comments".into())
                    .tab_stop(true)
                    .p_0()
                    .flex_shrink_0()
                    .h(px(theme::ATTACH_CHIP_H))
                    .px(px(theme::SPACE_0_5))
                    .rounded(px(theme::R_TIGHT))
                    .key_context("PromptAttachment")
                    .accessibility_label("Remove the review comments")
                    .tooltip("Remove the review comments")
                    .child(div().text_color(rgb(theme::TEXT_DIM)).child("\u{d7}"))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        on_drop(window, cx);
                    }),
            )
        })
}

impl RenderOnce for Attachments {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // The draft's shelf and the delivered prompt's files are one recipe;
        // only the draft can remove.
        chips(self)
    }
}

/// A file link in prose (the prototype's `.path`): the path as the prose
/// wrote it in `LINK_INK` with no chip, a `:line` suffix in `TEXT_DIM`. It
/// is a path target (`file_links::wire_inline`): a `LINK_INK` underline only
/// under the pointer, the hover card while it is there (hosted by `scope`,
/// the transcript it sits in), ⌘-click to the Thread's preview; a plain
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
    let size = gpui::size(
        inline_file_advance(&name, &location, link_w, size_px, window),
        line_h,
    );
    let host = preview.cloned();
    let selector = format!("file-attachment-{}", file.path.display());
    let accessibility = format!("Open {name}");
    let mut target = crate::file_links::PathTarget::new(file.path.clone())
        .at_line(
            file.location
                .as_deref()
                .and_then(crate::file_links::location_line),
        )
        .shown(gpui::SharedString::from(name.clone()));
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
    let link = gpui::div()
        .id("inline-file-chip")
        .flex()
        .items_center()
        .w(link_w)
        .h_full()
        .min_w_0()
        .font_family(theme::FONT_CODE)
        .font_weight(theme::W_BODY)
        .not_italic()
        .child(
            // The path gives way to an ellipsis; the `:line` never does.
            gpui::div()
                .flex()
                .min_w_0()
                .child(
                    gpui::div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme::LINK_INK))
                        .child(name),
                )
                .when(!location.is_empty(), |title| {
                    title.child(
                        gpui::div()
                            .flex_none()
                            .text_color(rgb(theme::TEXT_DIM))
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
            rgb(theme::LINK_INK).into(),
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
        .rounded(px(theme::R_CHIP))
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
