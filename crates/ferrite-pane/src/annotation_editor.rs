//! Feedback on retained evidence. This surface never owns a live BrowserView.
//! The local display cache is separate from the immutable Server receipt.
use crate::composer::{Composer, Edited, Role};
use ferrite_core::browser_annotations::{
    Annotation, Drawing, Ink, LiveScreenshot, Point as EvidencePoint, Rect, Selection, Viewport,
};
use ferrite_ui::{components, icons, theme};
use gpui::{
    canvas, div, point, prelude::*, px, rgb, size, AnyElement, App, AvailableSpace, Bounds,
    Context, Entity, EventEmitter, MouseButton, Pixels, Window,
};
use std::{cell::Cell, path::PathBuf, rc::Rc};

gpui::actions!(annotation_editor, [Cancel]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Line,
    Arrow,
    Rectangle,
    Ellipse,
    Step,
    Text,
}
impl Tool {
    fn label(self) -> &'static str {
        match self {
            Self::Pen => "Pen",
            Self::Line => "Line",
            Self::Arrow => "Arrow",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Step => "Steps",
            Self::Text => "Text",
        }
    }
}
pub enum EditorEvent {
    Added {
        comment: String,
        drawings: Vec<Drawing>,
    },
    Cancelled,
}
pub struct AnnotationEditor {
    retained: Annotation,
    local_image: PathBuf,
    comment: Entity<Composer>,
    label: Entity<Composer>,
    tool: Tool,
    ink: Ink,
    drawings: Vec<Drawing>,
    redo: Vec<Vec<Drawing>>,
    undo: Vec<Vec<Drawing>>,
    gesture: Option<Vec<EvidencePoint>>,
    picture: Rc<Cell<Option<Bounds<Pixels>>>>,
    options_expanded: bool,
}
impl EventEmitter<EditorEvent> for AnnotationEditor {}
impl gpui::Focusable for AnnotationEditor {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        gpui::Focusable::focus_handle(self.comment.read(cx), cx)
    }
}
impl AnnotationEditor {
    pub fn new(retained: Annotation, local_image: PathBuf, cx: &mut Context<Self>) -> Self {
        let comment = cx.new(|cx| {
            let mut field = Composer::new(cx);
            field.set_role(Role::Field(Some("AnnotationEditor")), cx);
            field.set_visible_row_limit(4, cx);
            field.set(retained.comment().into(), cx);
            field
        });
        cx.subscribe(&comment, |_, _, _: &Edited, cx| cx.notify())
            .detach();
        let label = cx.new(|cx| {
            let mut field = Composer::new(cx);
            field.set_role(Role::Field(Some("AnnotationEditor")), cx);
            field
        });
        Self {
            drawings: retained.drawings().to_vec(),
            retained,
            local_image,
            comment,
            label,
            tool: Tool::Pen,
            ink: Ink {
                color: format!("#{:06x}", theme::SEND_LIT),
                width: 3.,
            },
            redo: vec![],
            undo: vec![],
            gesture: None,
            picture: Rc::default(),
            options_expanded: false,
        }
    }
    pub fn select_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.gesture = None;
        self.tool = tool;
        if tool == Tool::Text {
            self.options_expanded = true;
        }
        cx.notify();
    }
    pub fn drawings(&self) -> &[Drawing] {
        &self.drawings
    }
    pub fn picture_bounds(&self) -> Option<Bounds<Pixels>> {
        self.picture.get()
    }
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        self.gesture = None;
        if let Some(previous) = self.undo.pop() {
            self.redo
                .push(std::mem::replace(&mut self.drawings, previous));
        }
        cx.notify();
    }
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        self.gesture = None;
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.drawings, next));
        }
        cx.notify();
    }
    fn checkpoint(&mut self) {
        self.undo.push(self.drawings.clone());
        self.redo.clear();
    }
    fn location(&self, at: gpui::Point<Pixels>) -> Option<EvidencePoint> {
        let bounds = self.picture.get()?;
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return None;
        }
        let viewport = &display_viewport(&self.retained);
        Some(EvidencePoint {
            x: (f32::from(at.x - bounds.origin.x) / f32::from(bounds.size.width)) as f64
                * viewport.width_css,
            y: (f32::from(at.y - bounds.origin.y) / f32::from(bounds.size.height)) as f64
                * viewport.height_css,
        })
        .map(|p| EvidencePoint {
            x: p.x.clamp(0., viewport.width_css),
            y: p.y.clamp(0., viewport.height_css),
        })
    }
    fn operation(&self, points: &[EvidencePoint], cx: &App) -> Option<Drawing> {
        let from = points.first()?.clone();
        let to = points.last()?.clone();
        let ink = self.ink.clone();
        let bounds = Rect {
            x: from.x.min(to.x),
            y: from.y.min(to.y),
            width: (from.x - to.x).abs(),
            height: (from.y - to.y).abs(),
        };
        Some(match self.tool {
            Tool::Pen => Drawing::Pen {
                points: points.to_vec(),
                ink,
            },
            Tool::Line => Drawing::Line { from, to, ink },
            Tool::Arrow => Drawing::Arrow { from, to, ink },
            Tool::Rectangle => Drawing::Rectangle { bounds, ink },
            Tool::Ellipse => Drawing::Ellipse { bounds, ink },
            Tool::Step => Drawing::Step {
                at: from,
                number: self
                    .drawings
                    .iter()
                    .filter_map(|d| {
                        if let Drawing::Step { number, .. } = d {
                            Some(*number)
                        } else {
                            None
                        }
                    })
                    .max()
                    .unwrap_or(0)
                    + 1,
                ink,
            },
            Tool::Text => {
                let text = self.label.read(cx).text().trim().to_owned();
                if text.is_empty() {
                    return None;
                }
                Drawing::Text {
                    at: from,
                    text,
                    ink,
                }
            }
        })
    }
    fn finish(&mut self, at: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let Some(mut points) = self.gesture.take() else {
            return;
        };
        if let Some(last) = self.location(at) {
            points.push(last);
        }
        if let Some(drawing) = self.operation(&points, cx) {
            self.checkpoint();
            self.drawings.push(drawing);
        }
        cx.notify();
    }
}
fn control(
    id: impl Into<gpui::ElementId>,
    selected: bool,
    cx: &App,
) -> gpui::component::button::Button {
    let rest = if selected {
        theme::Paint((theme::SEND_LIT << 8) | 26)
    } else {
        theme::Paint(theme::TRANSPARENT)
    };
    components::form_button_on(id, rest, cx)
        .h(px(28.))
        .px(px(8.))
        .rounded(px(6.))
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(theme::W_LABEL)
        .text_color(rgb(if selected {
            theme::SEND_LIT
        } else {
            theme::INK_STRONG
        }))
}

fn control_label(text: impl Into<gpui::SharedString>, size: f32, ink: u32) -> gpui::Div {
    div()
        .text_size(px(size))
        .line_height(px(if size == 14. { 20. } else { 16. }))
        .font_weight(theme::W_LABEL)
        .text_color(rgb(ink))
        .child(text.into())
}

fn control_group() -> gpui::Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(2.))
        .p(px(4.))
        .border_1()
        .border_color(rgb(theme::LINE))
        .rounded(px(8.))
        .bg(rgb(theme::CARD))
}

fn field() -> gpui::Div {
    div()
        .min_w_0()
        .min_h(px(28.))
        .px(px(8.))
        .py(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(rgb(theme::INPUT_EDGE))
        .bg(rgb(theme::FRAME))
}

fn feedback_bounds(selection: &Selection, drawings: &[Drawing]) -> Option<Rect> {
    let mut bounds: Option<Bounds<f64>> = None;
    let mut include = |x, y| {
        let next = Bounds {
            origin: point(x, y),
            size: size(0., 0.),
        };
        bounds = Some(bounds.map_or(next, |current| current.union(&next)));
    };
    match selection {
        Selection::Region {
            x,
            y,
            width,
            height,
        } => {
            include(*x, *y);
            include(x + width, y + height);
        }
        Selection::Element { context } => {
            let r = &context.bounds_css;
            include(r.x, r.y);
            include(r.x + r.width, r.y + r.height);
        }
        Selection::Live { feedback } => {
            if let LiveScreenshot::Captured { crop } = &feedback.screenshot {
                include(0., 0.);
                include(crop.width, crop.height);
            }
        }
        Selection::Unknown => {}
    }
    for drawing in drawings {
        match drawing {
            Drawing::Pen { points, .. } => {
                for p in points {
                    include(p.x, p.y);
                }
            }
            Drawing::Line { from, to, .. } | Drawing::Arrow { from, to, .. } => {
                include(from.x, from.y);
                include(to.x, to.y);
            }
            Drawing::Rectangle { bounds, .. } | Drawing::Ellipse { bounds, .. } => {
                include(bounds.x, bounds.y);
                include(bounds.x + bounds.width, bounds.y + bounds.height);
            }
            Drawing::Text { at, .. } | Drawing::Step { at, .. } => include(at.x, at.y),
            Drawing::Unknown => {}
        }
    }
    bounds.map(|bounds| Rect {
        x: bounds.origin.x,
        y: bounds.origin.y,
        width: bounds.size.width,
        height: bounds.size.height,
    })
}

fn editor_position(
    viewport: Bounds<Pixels>,
    target: Bounds<Pixels>,
    extent: gpui::Size<Pixels>,
) -> gpui::Point<Pixels> {
    let gap = px(8.);
    let candidates = [
        point(target.right() + gap, target.top()),
        point(target.left() - extent.width - gap, target.top()),
        point(target.right() - extent.width, target.bottom() + gap),
        point(
            target.right() - extent.width,
            target.top() - extent.height - gap,
        ),
    ];
    let overflow = |p: gpui::Point<Pixels>| {
        (viewport.left() - p.x).max(px(0.))
            + (viewport.top() - p.y).max(px(0.))
            + (p.x + extent.width - viewport.right()).max(px(0.))
            + (p.y + extent.height - viewport.bottom()).max(px(0.))
    };
    let best = candidates
        .into_iter()
        .reduce(|best, candidate| {
            if overflow(candidate) < overflow(best) {
                candidate
            } else {
                best
            }
        })
        .unwrap();
    point(
        best.x.clamp(
            viewport.left() + gap,
            (viewport.right() - extent.width - gap).max(viewport.left() + gap),
        ),
        best.y.clamp(
            viewport.top() + gap,
            (viewport.bottom() - extent.height - gap).max(viewport.top() + gap),
        ),
    )
}

impl Render for AnnotationEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tools = control_group().occlude();
        for tool in [
            Tool::Pen,
            Tool::Line,
            Tool::Arrow,
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Step,
            Tool::Text,
        ] {
            tools = tools.child(
                control(
                    gpui::SharedString::from(format!("annotation-tool-{}", tool.label())),
                    self.tool == tool,
                    cx,
                )
                .h(px(32.))
                .px(px(10.))
                .text_size(px(14.))
                .line_height(px(20.))
                .debug_selector(move || format!("annotation-tool-{}", tool.label()).into())
                .child(control_label(
                    tool.label(),
                    14.,
                    if self.tool == tool {
                        theme::SEND_LIT
                    } else {
                        theme::INK_STRONG
                    },
                ))
                .on_click(cx.listener(move |this, _, _, cx| this.select_tool(tool, cx))),
            );
        }
        let mut inks = control_group();
        for (name, color) in [
            ("Red", theme::RED),
            ("Blue", theme::SEND_LIT),
            ("Green", theme::GREEN),
            ("White", theme::INK_STRONG),
        ] {
            inks = inks.child(
                control(
                    gpui::SharedString::from(format!("annotation-ink-{name}")),
                    self.ink.color == format!("#{color:06x}"),
                    cx,
                )
                .debug_selector(move || format!("annotation-ink-{name}").into())
                .child(control_label(name, 12., color))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.ink.color = format!("#{color:06x}");
                    cx.notify();
                })),
            );
        }
        for width in [1, 3, 6] {
            inks = inks.child(
                control(
                    gpui::SharedString::from(format!("annotation-width-{width}")),
                    self.ink.width == width as f64,
                    cx,
                )
                .debug_selector(move || format!("annotation-width-{width}").into())
                .child(control_label(
                    format!("{width} px"),
                    12.,
                    if self.ink.width == width as f64 {
                        theme::SEND_LIT
                    } else {
                        theme::INK_STRONG
                    },
                ))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.ink.width = width as f64;
                    cx.notify();
                })),
            );
        }
        let mut drawings = self.drawings.clone();
        if let Some(operation) = self
            .gesture
            .as_ref()
            .and_then(|points| self.operation(points, cx))
        {
            drawings.push(operation);
        }
        let target = feedback_bounds(&self.retained.capture().selection, &drawings);
        let viewport = display_viewport(&self.retained);
        let editor = self.gesture.as_ref().map(|_| cx.entity().downgrade());
        let stage = stage(
            &self.retained,
            self.local_image.clone(),
            drawings,
            self.picture.clone(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                if this
                    .picture
                    .get()
                    .is_some_and(|bounds| bounds.contains(&event.position))
                {
                    this.gesture = this.location(event.position).map(|p| vec![p]);
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        )
        .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
            if this.gesture.is_some() {
                if let Some(p) = this.location(event.position) {
                    this.gesture.as_mut().unwrap().push(p);
                }
                cx.stop_propagation();
                cx.notify();
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                this.finish(event.position, cx);
                cx.stop_propagation();
            }),
        );
        let stage = stage.child(
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    if let Some(editor) = editor {
                        let moved = editor.clone();
                        window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Capture
                                && event.pressed_button == Some(MouseButton::Left)
                            {
                                moved
                                    .update(cx, |this, cx| {
                                        if let Some(p) = this.location(event.position) {
                                            if let Some(points) = this.gesture.as_mut() {
                                                points.push(p);
                                            }
                                        }
                                        cx.notify();
                                    })
                                    .ok();
                            }
                        });
                        window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Capture
                                && event.button == MouseButton::Left
                            {
                                editor
                                    .update(cx, |this, cx| this.finish(event.position, cx))
                                    .ok();
                            }
                        });
                    }
                },
            )
            .absolute()
            .inset_0(),
        );
        let history = div()
            .flex()
            .flex_wrap()
            .gap(px(2.))
            .child(
                control("annotation-undo", false, cx)
                    .child(control_label("Undo", 12., theme::INK_STRONG))
                    .on_click(cx.listener(|this, _, _, cx| this.undo(cx))),
            )
            .child(
                control("annotation-redo", false, cx)
                    .child(control_label("Redo", 12., theme::INK_STRONG))
                    .on_click(cx.listener(|this, _, _, cx| this.redo(cx))),
            )
            .child(
                control("annotation-clear", false, cx)
                    .child(control_label("Clear drawings", 12., theme::INK_STRONG))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.drawings.is_empty() {
                            this.checkpoint();
                            this.drawings.clear();
                        }
                        cx.notify();
                    })),
            )
            .child(
                control("annotation-cancel", false, cx)
                    .debug_selector(|| "annotation-cancel".into())
                    .child(control_label("Cancel", 12., theme::INK_STRONG))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Cancelled))),
            );
        let focused = gpui::Focusable::focus_handle(self.comment.read(cx), cx).is_focused(window);
        let comment = div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h(px(32.))
            .max_h(px(96.))
            .py(px(6.))
            .border_b_1()
            .border_color(if focused {
                rgb(theme::SEND_LIT).into()
            } else {
                gpui::Hsla::transparent_black()
            })
            .text_size(px(14.))
            .line_height(px(20.))
            .debug_selector(|| "annotation-comment".into())
            .when(self.comment.read(cx).is_empty(), |field| {
                field.child(
                    div()
                        .absolute()
                        .top(px(6.))
                        .left_0()
                        .text_color(rgb(theme::INK_SUBTLE))
                        .child("Describe the change…"),
                )
            })
            .child(self.comment.clone());
        let composer = div()
            .id("annotation-popover")
            .debug_selector(|| "annotation-popover".into())
            .occlude()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_y_scroll()
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(theme::LINE))
            .bg(rgb(theme::CARD))
            .shadow(components::float_shadow())
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .p(px(8.))
                    .child(
                        control("annotation-options", self.options_expanded, cx)
                            .debug_selector(|| "annotation-options".into())
                            .w(px(32.))
                            .h(px(32.))
                            .p_0()
                            .flex_shrink_0()
                            .child(icons::icon(icons::LIST_FILTER, 15., theme::INK_SUBTLE))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.options_expanded = !this.options_expanded;
                                cx.notify();
                            })),
                    )
                    .child(comment)
                    .child(
                        components::faded_button(
                            "annotation-add",
                            rgb(theme::SEND_LIT).into(),
                            rgb(theme::SEND_LIT_HOVER).into(),
                            rgb(theme::SEND_LIT_HOVER).into(),
                            rgb(theme::ON_SEND_LIT).into(),
                            cx,
                        )
                        .tab_stop(true)
                        .h(px(32.))
                        .px(px(12.))
                        .rounded(px(6.))
                        .flex_shrink_0()
                        .debug_selector(|| "annotation-add".into())
                        .child(control_label("Attach", 14., theme::ON_SEND_LIT))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.emit(EditorEvent::Added {
                                comment: this.comment.read(cx).text().to_owned(),
                                drawings: this.drawings.clone(),
                            });
                        })),
                    ),
            )
            .when(self.options_expanded, |popover| {
                popover.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .p(px(8.))
                        .border_t_1()
                        .border_color(rgb(theme::LINE))
                        .child(
                            inks.border_0()
                                .rounded_none()
                                .p_0()
                                .bg(gpui::Hsla::transparent_black()),
                        )
                        .when(self.tool == Tool::Text, |options| {
                            options
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .line_height(px(16.))
                                        .text_color(rgb(theme::INK_SUBTLE))
                                        .child("Text label"),
                                )
                                .child(
                                    field()
                                        .debug_selector(|| "annotation-label".into())
                                        .child(self.label.clone()),
                                )
                        })
                        .child(history),
                )
            });
        let picture = self.picture.clone();
        let chrome = canvas(
            move |bounds, window, cx| {
                let available_width = (bounds.size.width - px(16.)).max(px(1.));
                let mut tools = tools
                    .max_w(available_width)
                    .shadow(components::float_shadow())
                    .into_any_element();
                let toolbar_size = tools.layout_as_root(
                    size(AvailableSpace::MaxContent, AvailableSpace::MinContent),
                    window,
                    cx,
                );
                tools.prepaint_at(
                    bounds.origin + point((bounds.size.width - toolbar_size.width) / 2., px(10.)),
                    window,
                    cx,
                );
                let mut composer = composer
                    .w(px(360.).min(available_width))
                    .max_h((bounds.size.height - px(16.)).max(px(1.)))
                    .into_any_element();
                let extent = composer.layout_as_root(
                    size(AvailableSpace::MinContent, AvailableSpace::MinContent),
                    window,
                    cx,
                );
                let target = picture
                    .get()
                    .zip(target)
                    .map(|(picture, target)| {
                        Bounds::from_corners(
                            map(
                                &EvidencePoint {
                                    x: target.x,
                                    y: target.y,
                                },
                                picture,
                                &viewport,
                            ),
                            map(
                                &EvidencePoint {
                                    x: target.x + target.width,
                                    y: target.y + target.height,
                                },
                                picture,
                                &viewport,
                            ),
                        )
                    })
                    .unwrap_or(Bounds {
                        origin: bounds.center(),
                        size: size(px(0.), px(0.)),
                    });
                composer.prepaint_at(editor_position(bounds, target, extent), window, cx);
                (tools, composer)
            },
            |_, (mut tools, mut composer), window, cx| {
                tools.paint(window, cx);
                composer.paint(window, cx);
            },
        )
        .absolute()
        .inset_0();
        div()
            .id("annotation-editor")
            .debug_selector(|| "annotation-editor".into())
            .relative()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .key_context("AnnotationEditor")
            .on_action(cx.listener(|_, _: &Cancel, _, cx| {
                cx.emit(EditorEvent::Cancelled);
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(|_, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.emit(EditorEvent::Cancelled);
                    cx.stop_propagation();
                }
            }))
            .flex()
            .flex_col()
            .font_family(theme::FONT_UI)
            .text_size(px(14.))
            .line_height(px(20.))
            .text_color(rgb(theme::INK_STRONG))
            .child(stage)
            .child(chrome)
    }
}

fn display_viewport(annotation: &Annotation) -> Viewport {
    if let Selection::Live { feedback } = &annotation.capture().selection {
        if let LiveScreenshot::Captured { crop } = &feedback.screenshot {
            return Viewport {
                width_css: crop.width,
                height_css: crop.height,
                device_scale: annotation.capture().viewport.device_scale,
            };
        }
    }
    annotation.capture().viewport.clone()
}

pub fn annotation_context(annotation: &Annotation) -> AnyElement {
    let mut context = div().flex().flex_col().gap(px(theme::SPACE_2));
    if let Selection::Live { feedback } = &annotation.capture().selection {
        if let LiveScreenshot::Unavailable { reason } = &feedback.screenshot {
            context = context.child(format!("Screenshot unavailable. {reason}"));
        }
        for target in &feedback.elements {
            let label = target
                .element
                .component_name
                .as_deref()
                .unwrap_or(&target.element.tag_name);
            context = context.child(format!(
                "{label} · {}",
                target.element.selector.as_deref().unwrap_or("Element")
            ));
            if let Some(source) = &target.element.source {
                if let Some(file) = &source.file_name {
                    context = context.child(format!("{file}:{}", source.line_number.unwrap_or(1)));
                }
            }
        }
        if !feedback.regions.is_empty() {
            context = context.child(format!("{} region targets", feedback.regions.len()));
        }
        if !feedback.strokes.is_empty() {
            context = context.child(format!("{} drawn strokes", feedback.strokes.len()));
        }
        for change in &feedback.style_changes {
            context = context.child(format!(
                "{}: {} → {}",
                change.property, change.previous_value, change.value
            ));
        }
    }
    context.into_any_element()
}

/// Review the recorded selection and operations without consulting a live page.
pub fn annotation_stage(annotation: &Annotation, local_image: PathBuf) -> AnyElement {
    stage(
        annotation,
        local_image,
        annotation.drawings().to_vec(),
        Rc::default(),
    )
    .into_any_element()
}
fn stage(
    annotation: &Annotation,
    local_image: PathBuf,
    drawings: Vec<Drawing>,
    picture: Rc<Cell<Option<Bounds<Pixels>>>>,
) -> gpui::Div {
    let viewport = display_viewport(annotation);
    if !viewport.width_css.is_finite()
        || !viewport.height_css.is_finite()
        || viewport.width_css <= 0.
        || viewport.height_css <= 0.
    {
        picture.set(None);
        return div()
            .flex_1()
            .child("Capture geometry is unavailable. Capture the page again.");
    }
    if !local_image.is_file() {
        picture.set(None);
        return div()
            .flex_1()
            .child("The retained image is unavailable. Reopen the annotation to try again.");
    }
    let painted_selection = !matches!(annotation.capture().selection, Selection::Live { .. });
    let preview_drawings = drawings.clone();
    let selection = match &annotation.capture().selection {
        Selection::Region {
            x,
            y,
            width,
            height,
        } => Rect {
            x: *x,
            y: *y,
            width: *width,
            height: *height,
        },
        Selection::Element { context } => context.bounds_css.clone(),
        Selection::Live { .. } => Rect {
            x: 0.,
            y: 0.,
            width: viewport.width_css,
            height: viewport.height_css,
        },
        Selection::Unknown => {
            picture.set(None);
            return div()
                .flex_1()
                .child("This selection requires a newer Ferrite version.");
        }
    };
    div()
        .debug_selector(|| "annotation-stage".into())
        .relative()
        .flex_1()
        .min_h(px(theme::ROW * 4.))
        .w_full()
        .overflow_hidden()
        .child(
            canvas(
                move |slot, window, cx| {
                    let scale = (f32::from(slot.size.width) / viewport.width_css as f32)
                        .min(f32::from(slot.size.height) / viewport.height_css as f32);
                    let extent = size(
                        px(viewport.width_css as f32 * scale),
                        px(viewport.height_css as f32 * scale),
                    );
                    let bounds = Bounds {
                        origin: slot.center() - point(extent.width / 2., extent.height / 2.),
                        size: extent,
                    };
                    picture.set(Some(bounds));
                    let mut image = gpui::img(local_image)
                        .object_fit(gpui::ObjectFit::Fill)
                        .size_full()
                        .into_any_element();
                    image.prepaint_as_root(
                        bounds.origin,
                        extent.map(AvailableSpace::Definite),
                        window,
                        cx,
                    );
                    let mut labels = vec![];
                    for drawing in &preview_drawings {
                        let (at, text, ink) = match drawing {
                            Drawing::Text { at, text, ink } => (at, text.clone(), ink),
                            Drawing::Step { at, number, ink } => (at, number.to_string(), ink),
                            _ => continue,
                        };
                        let mut label = div()
                            .text_color(rgb(color(ink)))
                            .text_size(px(16. * scale))
                            .child(text)
                            .into_any_element();
                        label.prepaint_as_root(
                            map(at, bounds, &viewport),
                            size(AvailableSpace::MinContent, AvailableSpace::MinContent),
                            window,
                            cx,
                        );
                        labels.push(label);
                    }
                    (image, labels, bounds, viewport)
                },
                move |_, (mut image, labels, bounds, viewport), window, cx| {
                    image.paint(window, cx);
                    window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                        if painted_selection {
                            let selected = Bounds::from_corners(
                                map(
                                    &EvidencePoint {
                                        x: selection.x,
                                        y: selection.y,
                                    },
                                    bounds,
                                    &viewport,
                                ),
                                map(
                                    &EvidencePoint {
                                        x: selection.x + selection.width,
                                        y: selection.y + selection.height,
                                    },
                                    bounds,
                                    &viewport,
                                ),
                            );
                            let fill: gpui::Hsla = rgb(theme::SEND_LIT).into();
                            window.paint_quad(gpui::fill(selected, fill.opacity(0.1)));
                            paint(
                                &Drawing::Rectangle {
                                    bounds: selection.clone(),
                                    ink: Ink {
                                        color: format!("#{:06x}", theme::SEND_LIT),
                                        width: 2.,
                                    },
                                },
                                bounds,
                                &viewport,
                                window,
                            );
                        }
                        for drawing in &drawings {
                            paint(drawing, bounds, &viewport, window);
                        }
                        for mut label in labels {
                            label.paint(window, cx);
                        }
                    });
                },
            )
            .size_full(),
        )
}
fn color(ink: &Ink) -> u32 {
    u32::from_str_radix(ink.color.trim_start_matches('#'), 16).unwrap_or(theme::RED)
}
fn map(p: &EvidencePoint, bounds: Bounds<Pixels>, viewport: &Viewport) -> gpui::Point<Pixels> {
    bounds.origin
        + point(
            bounds.size.width * (p.x / viewport.width_css) as f32,
            bounds.size.height * (p.y / viewport.height_css) as f32,
        )
}
fn paint(drawing: &Drawing, bounds: Bounds<Pixels>, viewport: &Viewport, window: &mut Window) {
    let (points, ink): (Vec<EvidencePoint>, &Ink) = match drawing {
        Drawing::Pen { points, ink } => (points.clone(), ink),
        Drawing::Line { from, to, ink } | Drawing::Arrow { from, to, ink } => {
            let mut points = vec![from.clone(), to.clone()];
            if matches!(drawing, Drawing::Arrow { .. }) {
                let angle = (to.y - from.y).atan2(to.x - from.x);
                let length = 10. + ink.width * 2.;
                for turn in [-0.5, 0.5] {
                    points.push(EvidencePoint {
                        x: to.x - length * (angle + turn).cos(),
                        y: to.y - length * (angle + turn).sin(),
                    });
                    points.push(to.clone());
                }
            }
            (points, ink)
        }
        Drawing::Rectangle { bounds: r, ink } => (
            vec![
                EvidencePoint { x: r.x, y: r.y },
                EvidencePoint {
                    x: r.x + r.width,
                    y: r.y,
                },
                EvidencePoint {
                    x: r.x + r.width,
                    y: r.y + r.height,
                },
                EvidencePoint {
                    x: r.x,
                    y: r.y + r.height,
                },
                EvidencePoint { x: r.x, y: r.y },
            ],
            ink,
        ),
        Drawing::Ellipse { bounds: r, ink } => (
            (0..=64)
                .map(|i| {
                    let a = i as f64 / 64. * std::f64::consts::TAU;
                    EvidencePoint {
                        x: r.x + r.width / 2. + r.width / 2. * a.cos(),
                        y: r.y + r.height / 2. + r.height / 2. * a.sin(),
                    }
                })
                .collect(),
            ink,
        ),
        Drawing::Text { .. } | Drawing::Step { .. } | Drawing::Unknown => return,
    };
    let mut path =
        gpui::PathBuilder::stroke(bounds.size.width * (ink.width / viewport.width_css) as f32);
    for (index, p) in points.iter().enumerate() {
        if index == 0 {
            path.move_to(map(p, bounds, viewport));
        } else {
            path.line_to(map(p, bounds, viewport));
        }
    }
    if let Ok(path) = path.build() {
        let color: gpui::Hsla = rgb(color(ink)).into();
        window.paint_path(path, color);
    }
}
#[cfg(test)]
mod tests;
