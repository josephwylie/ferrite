//! The same kit attachment cards in the draft and delivered prompt. This
//! module owns presentation and image preview; callers only supply paths
//! and, for a draft, a removal callback. The prompt codec owns persistence.

use std::{path::PathBuf, rc::Rc, time::Duration};

use gpui::base::motion::{animate_keyframes, Easing, Keyframe, Keyframes, Timing};
use gpui::component::{
    attachment::{
        Attachment, AttachmentActions, AttachmentContent, AttachmentDescription, AttachmentGroup,
        AttachmentMedia, AttachmentTitle,
    },
    button::{Button, ButtonVariants},
    Icon, IconName, Sizable, Theme,
};
use gpui::{prelude::*, px, App, Axis, ElementId, Global, IntoElement, Window};

use crate::attachment_preview::Preview;

// Keep the actual kit defaults before Ferrite makes its global borders
// transparent. Restoring these tokens on the kit slots preserves the
// documented attachment surface without changing the rest of the app.
struct Appearance(Theme);
impl Global for Appearance {}

pub fn init(cx: &mut App) {
    cx.set_global(Appearance(Theme::global(cx).clone()));
}

type Remove = Rc<dyn Fn(&PathBuf, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Attachments {
    id: ElementId,
    files: Vec<PathBuf>,
    preview: Preview,
    on_remove: Option<Remove>,
    island: Option<usize>,
}

impl Attachments {
    pub fn new(id: impl Into<ElementId>, files: Vec<PathBuf>, preview: &Preview) -> Self {
        Self {
            id: id.into(),
            files,
            preview: preview.clone(),
            on_remove: None,
            island: None,
        }
    }

    /// Pending attachments as compact chips on the shelf above the prompt.
    pub fn in_island(mut self, generation: usize) -> Self {
        self.island = Some(generation);
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

/// The pending files as chips on the shelf above the Composer — the
/// background chips' recipe, so files going in and work going on read as
/// one surface: `ATTACH_CHIP_H`, `R_CHIP`, `FILL` (stepping to `FILL_HOVER`
/// under the pointer), a 16px thumbnail or file mark, the name in UI type
/// `FS_SM` `TEXT_2` cut at `ATTACH_CHIP_MAX_W`, and a quiet `×`. The image
/// thumbnail and the `×` are real buttons (tab stops, Enter/Space) in the
/// `PromptAttachment` key context; a click anywhere else on a chip opens
/// the image preview or the file. A new set eases in over 140ms.
fn pending_chips(
    attachments: Attachments,
    generation: usize,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    use crate::pointer::Pointer as _;
    use crate::theme;
    use gpui::{div, rgb, SharedString};
    // The kit retains playback by generation and honors reduced motion.
    // Typing and image-loading repaints continue the same entrance.
    let entrance = animate_keyframes(
        ElementId::from(("attachment-island-enter", generation)),
        &Keyframes::try_new([Keyframe::new(0., 0_f32), Keyframe::new(1., 1.)])
            .expect("two ordered entrance keyframes"),
        Timing::new(Duration::from_millis(140)).ease(Easing::EaseOut),
        window,
        cx,
    )
    .value;
    let Attachments {
        id,
        files,
        preview,
        on_remove,
        ..
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
                .size(gpui::px(theme::ATTACH_THUMB))
                .rounded(gpui::px(theme::R_TIGHT))
                .key_context("PromptAttachment")
                .accessibility_label(format!("Preview {name}"))
                .tooltip("Preview image")
                .child(
                    gpui::img(path.clone())
                        .size(gpui::px(theme::ATTACH_THUMB))
                        .rounded(gpui::px(theme::R_TIGHT))
                        .object_fit(gpui::ObjectFit::Cover),
                )
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
                .size(gpui::px(theme::ATTACH_THUMB))
                .child(
                    Icon::new(IconName::File)
                        .size(gpui::px(theme::ICON_CHEVRON))
                        .text_color(rgb(theme::TEXT_MUTED)),
                )
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
            .gap(gpui::px(theme::SPACE_1_5))
            .h(gpui::px(theme::ATTACH_CHIP_H))
            .max_w(gpui::px(theme::ATTACH_CHIP_MAX_W))
            .min_w_0()
            .pl(gpui::px(theme::SPACE_0_5))
            .pr(gpui::px(theme::SPACE_0_5))
            .rounded(gpui::px(theme::R_CHIP))
            .bg(rgb(theme::FILL))
            .hover_carried()
            .font_family(theme::FONT_UI)
            .text_size(gpui::px(theme::FS_SM))
            .line_height(gpui::px(theme::LH_META))
            .text_color(rgb(theme::TEXT_2))
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
                        .size(gpui::px(theme::BG_CHIP_STOP))
                        .rounded(gpui::px(theme::R_TIGHT))
                        .key_context("PromptAttachment")
                        .accessibility_label(format!("Remove {name}"))
                        .tooltip(format!("Remove {}", path.display()))
                        .child(crate::icons::icon(
                            crate::icons::CLOSE,
                            theme::BG_CHIP_STOP_GLYPH,
                            theme::TEXT_MUTED,
                        ))
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
        .debug_selector(|| "attachment-island-content".into())
        .flex()
        .flex_wrap()
        .items_center()
        .gap(gpui::px(theme::SPACE_1_5))
        .min_w_0()
        .max_w_full()
        .relative()
        .top(gpui::px(theme::SPACE_1 * (1. - entrance)))
        .opacity(0.6 + 0.4 * entrance)
        .children(chips)
        .into_any_element()
}

impl RenderOnce for Attachments {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if let Some(generation) = self.island {
            return pending_chips(self, generation, window, cx);
        }
        // A delivered prompt's files: the kit cards, on the kit's own stock
        // tokens (the prompt row owns their placement).
        let stock = &cx.global::<Appearance>().0;
        let tokens = stock.semantic_tokens();
        let cards = AttachmentGroup::new(self.id)
            .font_family(stock.font_family.clone())
            .children(self.files.into_iter().enumerate().map(|(index, path)| {
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
                let preview = path.clone();
                let title = name.clone();
                let button_preview = path.clone();
                let button_title = name.clone();
                let card_host = self.preview.clone();
                let button_host = self.preview.clone();
                let media = AttachmentMedia::new()
                    .bg(tokens.colors.muted)
                    .text_color(tokens.colors.foreground)
                    .rounded(tokens.radius.md);
                let card = Attachment::new()
                    .id(("attachment", index))
                    .bg(tokens.colors.background)
                    .text_color(tokens.colors.foreground)
                    .border_color(tokens.colors.border)
                    .rounded(stock.radius_2xl())
                    .axis(if image {
                        Axis::Vertical
                    } else {
                        Axis::Horizontal
                    })
                    .media(if image {
                        media.src(path.clone()).overlay(
                            Button::new(("preview-attachment", index))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Maximize)
                                .key_context("PromptAttachment")
                                .accessibility_label(format!("Preview {name}"))
                                .tooltip("Preview image")
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    button_host.open(
                                        button_preview.clone(),
                                        button_title.clone(),
                                        window,
                                        cx,
                                    );
                                }),
                        )
                    } else {
                        media.child(Icon::new(IconName::File))
                    })
                    .content(
                        AttachmentContent::new()
                            .title(AttachmentTitle::new(name.clone()))
                            .description(
                                AttachmentDescription::new("Attached")
                                    .text_color(tokens.colors.muted_foreground),
                            ),
                    )
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        if image {
                            card_host.open(preview.clone(), title.clone(), window, cx);
                        } else {
                            crate::file_links::FileLink {
                                path: preview.clone(),
                                location: None,
                            }
                            .open(window, cx);
                        }
                    })
                    .when_some(self.on_remove.clone(), |attachment, remove| {
                        attachment.actions(
                            AttachmentActions::new().child(
                                Button::new(("remove-attachment", index))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Close)
                                    .key_context("PromptAttachment")
                                    .accessibility_label(format!("Remove {name}"))
                                    .tooltip(format!("Remove {}", path.display()))
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        remove(&path, window, cx);
                                    }),
                            ),
                        )
                    });
                card
            }));
        KitScale {
            child: cards.into_any_element(),
            rem_size: stock.font_size,
        }
        .into_any_element()
    }
}

/// A file link in prose, drawn as inline code is: the name in the code face
/// at `FS_UI` in `TEXT` and a `:line` suffix in `TEXT_MUTED`, on the neutral
/// `INLINE_CODE_WASH` chip at `R_CHIP` (`FILL` under the pointer). An image
/// leads with its own thumbnail; any other file has no mark. The native
/// Markdown flow reserves the returned size — the chip plus
/// `INLINE_CODE_OVERHANG` after it, so a following `.` or `,` sits 2px off
/// — and wraps the chip atomically, so the width is measured in the face and
/// size it is drawn in: the name and the location each shaped whole.
pub fn inline_file(
    file: crate::file_links::FileLink,
    label: &str,
    preview: Option<&Preview>,
    window: &mut Window,
    cx: &mut App,
) -> (gpui::Size<gpui::Pixels>, gpui::AnyElement) {
    use crate::pointer::{Pointer as _, PointerPressed as _};
    use crate::theme;
    use gpui::{rgb, rgba};

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
    let chip_w = inline_file_width(&name, &location, image, window);
    // The flow reserves the chip and its trailing margin; the chip draws at
    // its own width inside that.
    let size = gpui::size(
        chip_w + px(theme::INLINE_CODE_OVERHANG),
        px(theme::INLINE_FILE_H),
    );
    let host = preview.cloned();
    let name_for_open = name.clone();
    let tooltip = format!("{label}\n{}", file.path.display());
    let selector = format!("file-attachment-{}", file.path.display());
    let accessibility = format!("Open {name}");
    let thumbnail = file.path.clone();
    let open = move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
        gpui::base::TextSelection::end(window, cx);
        cx.stop_propagation();
        if image && file.path.exists() {
            if let Some(host) = &host {
                host.open(file.path.clone(), name_for_open.clone(), window, cx);
                return;
            }
        }
        file.open(window, cx);
    };
    let chip = gpui::div()
        .id("inline-file-chip")
        .flex()
        .items_center()
        .w(chip_w)
        .h_full()
        .min_w_0()
        .px(px(theme::INLINE_FILE_PAD_X))
        .gap(px(theme::INLINE_FILE_GAP))
        .bg(rgba(theme::INLINE_CODE_WASH))
        .hover_raised()
        .press_raised()
        .rounded(px(theme::R_CHIP))
        .font_family(theme::FONT_CODE)
        .font_weight(theme::W_BODY)
        .not_italic()
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .when(image, |chip| {
            chip.child(
                gpui::img(thumbnail)
                    .flex_none()
                    .size(px(theme::INLINE_FILE_THUMB))
                    .rounded(px(theme::R_TIGHT)),
            )
        })
        .child(
            // The name gives way to an ellipsis; the `:line` never does.
            gpui::div()
                .flex()
                .min_w_0()
                .child(
                    gpui::div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme::TEXT))
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
    // The click and keyboard target lies over the chip and draws nothing but
    // the focus ring: the chip itself wears the hover face.
    let clear: gpui::Hsla = gpui::transparent_black();
    let target = crate::components::button("inline-file-action")
        .custom(
            gpui::component::button::ButtonCustomVariant::new(cx)
                .color(clear)
                .hover(clear)
                .active(clear),
        )
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
            .relative()
            .w(chip_w)
            .h(size.height)
            .cursor_pointer()
            .tooltip(move |window, cx| {
                gpui::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(chip)
            .child(target)
            .into_any_element(),
    )
}

/// An inline file chip's width: its padding, an image's thumbnail and gap,
/// and the name and `:line` each shaped whole in the code face at `FS_UI`,
/// rounded up to whole pixels with 1px to spare, so a name that fits is
/// never ellipsized. Clamped to `INLINE_FILE_MIN_W`…`INLINE_FILE_MAX_W`.
pub(crate) fn inline_file_width(
    name: &str,
    location: &str,
    image: bool,
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
                px(theme::FS_UI),
                &[face.to_run(text.len())],
                None,
            )
            .width()
            .ceil()
    };
    let text_w = width(name) + width(location) + px(1.);
    let chrome = 2. * theme::INLINE_FILE_PAD_X
        + if image {
            theme::INLINE_FILE_THUMB + theme::INLINE_FILE_GAP
        } else {
            0.
        };
    (text_w + px(chrome)).clamp(px(theme::INLINE_FILE_MIN_W), px(theme::INLINE_FILE_MAX_W))
}

/// Concave shoulders turn the kit container's sides into the prompt's top
/// edge. Only the join is drawn here; cards and their surface remain kit UI.
/// Root sets a smaller rem for Ferrite's compact controls. Scope the kit's
/// original rem to this subtree in every drawing phase, including image and
/// button layout. No attachment dimensions are duplicated here.
struct KitScale {
    child: gpui::AnyElement,
    rem_size: gpui::Pixels,
}

impl IntoElement for KitScale {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl gpui::Element for KitScale {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, ()) {
        (
            window.with_rem_size(Some(self.rem_size), |window| {
                self.child.request_layout(window, cx)
            }),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(self.rem_size), |window| {
            self.child.prepaint(window, cx);
        });
    }
    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(self.rem_size), |window| self.child.paint(window, cx));
    }
}
