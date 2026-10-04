//! The files a prompt carries, drawn the same way in the draft and in the
//! delivered prompt: the prototype's `.att` chip (theme WP-D) — one row on
//! `paint::BAND2`, a cell of padding each side, the image mark (or the file
//! mark) and the name in `PATH_INK`. This module owns presentation and
//! image preview; callers only supply paths and, for a draft, a removal
//! callback. The prompt codec owns persistence.

use std::{path::PathBuf, rc::Rc};

use gpui::{prelude::*, px, App, ElementId, IntoElement, Window};

use crate::attachment_preview::Preview;

/// Nothing to install now: delivered files draw in the float grammar rather
/// than on the kit's stock attachment cards, so the kit's own tokens are no
/// longer captured. Kept for the theme's init order (`init_components`).
pub fn init(_cx: &mut App) {}

type Remove = Rc<dyn Fn(&PathBuf, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Attachments {
    id: ElementId,
    files: Vec<PathBuf>,
    preview: Preview,
    on_remove: Option<Remove>,
    island: bool,
}

impl Attachments {
    pub fn new(id: impl Into<ElementId>, files: Vec<PathBuf>, preview: &Preview) -> Self {
        Self {
            id: id.into(),
            files,
            preview: preview.clone(),
            on_remove: None,
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
}

/// The files as `.att` chips, wrapping: one row on `paint::BAND2`, a cell of
/// padding each side, a mark — an image's is a real button (a tab stop,
/// Enter/Space) that opens the preview; any other file's the `FILE` mark —
/// then the name in `PATH_INK` cut at `ATTACH_CHIP_MAX_W` (a file name is
/// machine text), and, on a draft, a quiet `×` that removes it (a button
/// too). The buttons sit in the `PromptAttachment` key context, so their
/// Enter never sends. A click anywhere else on a chip opens the image
/// preview or the file.
fn chips(attachments: Attachments) -> gpui::AnyElement {
    use crate::pointer::Pointer as _;
    use crate::theme;
    use gpui::{div, rgb, SharedString};
    let Attachments {
        id,
        files,
        preview,
        on_remove,
        island,
    } = attachments;
    let chips = files.into_iter().enumerate().map(|(index, path)| {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let image = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                gpui::Img::extensions().contains(&ext.to_ascii_lowercase().as_str())
            });
        let mark = if image {
            let host = preview.clone();
            let open = path.clone();
            let title = name.clone();
            crate::components::button(("preview-attachment", index))
                .tab_stop(true)
                .p_0()
                .size(px(theme::ATTACH_THUMB))
                .rounded(px(theme::R_TIGHT))
                .key_context("PromptAttachment")
                .accessibility_label(format!("Preview {name}"))
                .tooltip("Preview image")
                .child(crate::icons::icon(
                    crate::icons::IMAGE,
                    theme::ATTACH_THUMB,
                    theme::PATH_INK,
                ))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    host.open(open.clone(), title.clone(), window, cx);
                })
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(theme::ATTACH_THUMB))
                .child(crate::icons::icon(
                    crate::icons::FILE,
                    theme::ATTACH_THUMB,
                    theme::PATH_INK,
                ))
                .into_any_element()
        };
        let host = preview.clone();
        let open = path.clone();
        let title = name.clone();
        div()
            .id(("attachment", index))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::CH))
            .h(px(theme::ATTACH_CHIP_H))
            .max_w(px(theme::ATTACH_CHIP_MAX_W))
            .min_w_0()
            .px(px(theme::CH))
            .bg(theme::paint::BAND2)
            .cursor_pointer()
            .hover_carried(format!("attachment-{index}-{}", open.display()))
            .font_family(theme::FONT_CODE)
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
            .font_weight(theme::W_BODY)
            .text_color(rgb(theme::PATH_INK))
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
                    crate::components::button(("remove-attachment", index))
                        .tab_stop(true)
                        .p_0()
                        .flex_shrink_0()
                        .h(px(theme::ATTACH_CHIP_H))
                        .px(px(theme::SPACE_0_5))
                        .rounded(px(theme::R_TIGHT))
                        .key_context("PromptAttachment")
                        .accessibility_label(format!("Remove {name}"))
                        .tooltip(format!("Remove {}", path.display()))
                        .child(div().text_color(rgb(theme::TEXT_MUTED)).child("\u{d7}"))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            remove(&removed, window, cx);
                        }),
                )
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                if image {
                    host.open(open.clone(), title.clone(), window, cx);
                } else {
                    crate::file_links::FileLink {
                        path: open.clone(),
                        location: None,
                    }
                    .open(window, cx);
                }
            })
    });
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
        .into_any_element()
}

impl RenderOnce for Attachments {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // The draft's shelf and the delivered prompt's files are one recipe;
        // only the draft can remove.
        chips(self)
    }
}

/// A file link in prose (the prototype's `.path`): the name in `PATH_INK`
/// with no chip, a `:line` suffix in `TEXT_MUTED`, and a cyan underline
/// only under the pointer. The native Markdown flow reserves the returned
/// size and wraps the link atomically, so the width is measured in the face
/// and size it is drawn in (the prose's own): the name and the location
/// each shaped whole.
pub fn inline_file(
    file: crate::file_links::FileLink,
    label: &str,
    preview: Option<&Preview>,
    window: &mut Window,
    cx: &mut App,
) -> (gpui::Size<gpui::Pixels>, gpui::AnyElement) {
    use crate::theme;
    use gpui::rgb;

    let name = file
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let extension = file
        .path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let location = file
        .location
        .as_ref()
        .map(|line| format!(":{line}"))
        .unwrap_or_default();
    let image = gpui::Img::extensions().contains(&extension.as_str());
    let size_px = window.text_style().font_size.to_pixels(window.rem_size());
    let link_w = inline_file_width(&name, &location, image, size_px, window);
    let line_h = window.line_height();
    let size = gpui::size(link_w, line_h);
    let host = preview.cloned();
    let name_for_open = name.clone();
    let tooltip = format!("{label}\n{}", file.path.display());
    let selector = format!("file-attachment-{}", file.path.display());
    let accessibility = format!("Open {name}");
    let open = move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
        gpui::base::TextSelection::end(window, cx);
        cx.stop_propagation();
        if image && file.path.exists() {
            if let Some(host) = &host {
                host.open(file.path.clone(), name_for_open.clone(), window, cx);
                return;
            }
        }
        if !image && file.path.exists() {
            if let Some(host) = &host {
                if host.open_text_document(file.path.clone(), name_for_open.clone(), window, cx) {
                    return;
                }
            }
        }
        file.open(window, cx);
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
            // The name gives way to an ellipsis; the `:line` never does.
            gpui::div()
                .flex()
                .min_w_0()
                .child(
                    gpui::div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme::PATH_INK))
                        .group_hover("inline-file", |style| {
                            style
                                .underline()
                                .text_decoration_color(rgb(theme::PATH_INK))
                        })
                        .child(name),
                )
                .when(!location.is_empty(), |title| {
                    title.child(
                        gpui::div()
                            .flex_none()
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child(location),
                    )
                }),
        );
    // The click and keyboard target lies over the link and draws nothing but
    // the focus ring.
    let clear: gpui::Hsla = gpui::transparent_black();
    let target = crate::components::button("inline-file-action")
        .custom(crate::pointer::button_variant(
            clear,
            rgb(theme::PATH_INK).into(),
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
    (
        size,
        gpui::div()
            .id("inline-file")
            .debug_selector(move || selector.clone())
            .group("inline-file")
            .relative()
            .w(link_w)
            .h(size.height)
            .cursor_pointer()
            .tooltip(move |window, cx| {
                gpui::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(link)
            .child(target)
            .into_any_element(),
    )
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
    use crate::theme;
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
