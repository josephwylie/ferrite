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
    group_box::{GroupBox, GroupBoxVariants},
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

    /// Compact pending attachments in a kit surface above the prompt.
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

impl RenderOnce for Attachments {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // The kit retains playback by generation and honors reduced motion.
        // Typing and image-loading repaints continue the same entrance.
        let entrance = self.island.map(|generation| {
            animate_keyframes(
                ElementId::from(("attachment-island-enter", generation)),
                &Keyframes::try_new([Keyframe::new(0., 0_f32), Keyframe::new(1., 1.)])
                    .expect("two ordered entrance keyframes"),
                Timing::new(Duration::from_millis(140)).ease(Easing::EaseOut),
                window,
                cx,
            )
            .value
        });
        let stock = &cx.global::<Appearance>().0;
        let tokens = stock.semantic_tokens();
        let cards = AttachmentGroup::new(self.id)
            .when(self.island.is_some(), |group| {
                group.w_auto().max_w_full().gap_1p5().py_0()
            })
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
                    .rounded(if self.island.is_some() {
                        tokens.radius.sm
                    } else {
                        tokens.radius.md
                    });
                Attachment::new()
                    .id(("attachment", index))
                    .when(self.island.is_some(), |attachment| {
                        attachment.xsmall().min_w_0().w_32()
                    })
                    .bg(tokens.colors.background)
                    .text_color(tokens.colors.foreground)
                    .border_color(tokens.colors.border)
                    .rounded(if self.island.is_some() {
                        tokens.radius.xl
                    } else {
                        stock.radius_2xl()
                    })
                    .axis(if image && self.island.is_none() {
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
                    })
            }));
        KitScale {
            child: if let Some(entrance) = entrance {
                let background = Theme::global(cx).muted;
                let radius = Theme::global(cx).radius_2xl();
                let surface = gpui::div()
                    .bg(background)
                    .rounded_tl(radius)
                    .rounded_tr(radius)
                    .rounded_bl(px(0.))
                    .rounded_br(px(0.))
                    .p_1p5()
                    .min_w_0()
                    .style()
                    .clone();
                gpui::div()
                    .flex()
                    .justify_center()
                    .min_w_0()
                    .px(radius)
                    .child(
                        gpui::div()
                            .debug_selector(|| "attachment-island-content".into())
                            .min_w_0()
                            .max_w_full()
                            .relative()
                            .top(px(8. * (1. - entrance)))
                            .opacity(0.6 + 0.4 * entrance)
                            .child(crate::components::composer_join(radius, background))
                            .child(
                                GroupBox::new()
                                    .id("attachment-island")
                                    .fill()
                                    .w_auto()
                                    .max_w_full()
                                    .content_style(surface)
                                    .child(cards),
                            ),
                    )
                    .into_any_element()
            } else {
                cards.into_any_element()
            },
            rem_size: stock.font_size,
        }
    }
}

/// The existing Attachment family, reduced to a single transcript-height row.
/// The native Markdown flow reserves this size and wraps the card atomically.
pub fn inline_file(
    file: crate::file_links::FileLink,
    label: &str,
    preview: Option<&Preview>,
    window: &mut Window,
    cx: &mut App,
) -> (gpui::Size<gpui::Pixels>, gpui::AnyElement) {
    let stock = &cx.global::<Appearance>().0;
    let tokens = stock.semantic_tokens();
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
        .to_ascii_uppercase();
    let kind = if extension.is_empty() || extension.len() > 8 {
        "FILE".to_string()
    } else {
        extension
    };
    let location = file
        .location
        .as_ref()
        .map(|line| format!(":{line}"))
        .unwrap_or_default();
    let title = format!("{name}{location}");
    let image = gpui::Img::extensions().contains(&kind.to_ascii_lowercase().as_str());
    let font_size = px(crate::theme::FS_MD);
    let run = window.text_style().to_run(title.len() + kind.len());
    let width = window
        .text_system()
        .shape_line(format!("{title}{kind}").into(), font_size, &[run], None)
        .width()
        + px(46.);
    let size = gpui::size(width.clamp(px(88.), px(240.)), px(26.));
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
    let card = Attachment::new()
        .id("inline-file-open")
        .xsmall()
        .axis(Axis::Horizontal)
        .w_full()
        .min_w_0()
        .h(size.height)
        .py_0()
        .px(px(4.))
        .bg(tokens.colors.background)
        .text_color(tokens.colors.foreground)
        .border_color(tokens.colors.border)
        .rounded(tokens.radius.sm)
        .font_family(crate::theme::FONT_UI)
        .text_size(font_size)
        .media(
            AttachmentMedia::new()
                .size(px(16.))
                .rounded(tokens.radius.sm)
                .bg(tokens.colors.muted)
                .text_color(tokens.colors.foreground)
                .map(|media| {
                    if image {
                        media.src(thumbnail.clone())
                    } else {
                        media.child(Icon::new(IconName::FileText).size(px(13.)))
                    }
                }),
        )
        .content(
            AttachmentContent::new()
                .min_w_0()
                .title(AttachmentTitle::new(title).text_size(font_size)),
        )
        .actions(
            AttachmentActions::new().child(
                gpui::div()
                    .text_size(px(9.))
                    .text_color(tokens.colors.muted_foreground)
                    .child(kind),
            ),
        );
    let rem_size = stock.font_size;
    (
        size,
        KitScale {
            child: gpui::div()
                .id("inline-file")
                .debug_selector(move || selector.clone())
                .relative()
                .w_full()
                .h(size.height)
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .child(card)
                .child(
                    Button::new("inline-file-action")
                        .ghost()
                        .absolute()
                        .inset_0()
                        .size_full()
                        .min_w_0()
                        .p_0()
                        .key_context("PromptAttachment")
                        .accessibility_label(accessibility)
                        .on_click(open),
                )
                .into_any_element(),
            rem_size,
        }
        .into_any_element(),
    )
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
