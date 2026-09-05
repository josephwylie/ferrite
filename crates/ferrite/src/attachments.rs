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
use gpui::{
    canvas, point, prelude::*, px, App, Axis, ElementId, Global, Hsla, IntoElement, PathBuilder,
    Pixels, Window,
};

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
                    .when(image, |attachment| {
                        attachment.on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            card_host.open(preview.clone(), title.clone(), window, cx);
                        })
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
                            .child(composer_join(radius, background))
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

/// Concave shoulders turn the kit container's sides into the prompt's top
/// edge. Only the join is drawn here; cards and their surface remain kit UI.
fn composer_join(radius: Pixels, background: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let left = bounds.origin.x;
            let right = bounds.right();
            let top = bounds.origin.y;
            let bottom = bounds.bottom();
            let mut path = PathBuilder::fill();
            path.move_to(point(left, bottom));
            path.curve_to(point(left + radius, top), point(left + radius, bottom));
            path.line_to(point(left + radius, bottom));
            path.close();
            path.move_to(point(right - radius, top));
            path.curve_to(point(right, bottom), point(right - radius, bottom));
            path.line_to(point(right - radius, bottom));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, background);
            }
        },
    )
    .absolute()
    .left(-radius)
    .right(-radius)
    .bottom_0()
    .h(radius)
}

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
