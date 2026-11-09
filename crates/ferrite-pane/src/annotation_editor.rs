//! Feedback on retained evidence. This surface never owns a live BrowserView.
//! The local display cache is separate from the immutable Server receipt.
use crate::composer::{Composer, Role};
use ferrite_core::browser_annotations::{
    Annotation, Drawing, Ink, Point as EvidencePoint, Rect, Selection, Viewport,
};
use ferrite_ui::{components, theme};
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
}
impl EventEmitter<EditorEvent> for AnnotationEditor {}
impl gpui::Focusable for AnnotationEditor {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.comment.read(cx).focus_handle(cx)
    }
}
impl AnnotationEditor {
    pub fn new(retained: Annotation, local_image: PathBuf, cx: &mut Context<Self>) -> Self {
        let comment = cx.new(|cx| {
            let mut field = Composer::new(cx);
            field.set_role(Role::Field(Some("AnnotationEditor")), cx);
            field.set(retained.comment().into(), cx);
            field
        });
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
                color: format!("#{:06x}", theme::RED),
                width: 3.,
            },
            redo: vec![],
            undo: vec![],
            gesture: None,
            picture: Rc::default(),
        }
    }
    pub fn select_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.gesture = None;
        self.tool = tool;
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
        let viewport = &self.retained.capture().viewport;
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
impl Render for AnnotationEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tools = div().flex().flex_wrap().gap(px(theme::SPACE_1));
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
                components::form_button(
                    gpui::SharedString::from(format!("annotation-tool-{}", tool.label())),
                    cx,
                )
                .debug_selector(move || format!("annotation-tool-{}", tool.label()).into())
                .child(components::form_label(
                    tool.label(),
                    if self.tool == tool {
                        theme::BLUE
                    } else {
                        theme::INK_STRONG
                    },
                ))
                .on_click(cx.listener(move |this, _, _, cx| this.select_tool(tool, cx))),
            );
        }
        let mut inks = div().flex().flex_wrap().gap(px(theme::SPACE_1));
        for (name, color) in [
            ("Red", theme::RED),
            ("Blue", theme::BLUE),
            ("Green", theme::GREEN),
            ("White", theme::INK_STRONG),
        ] {
            inks = inks.child(
                components::form_button_on(
                    gpui::SharedString::from(format!("annotation-ink-{name}")),
                    if self.ink.color == format!("#{color:06x}") {
                        theme::paint::SELECTED
                    } else {
                        theme::Paint(theme::TRANSPARENT)
                    },
                    cx,
                )
                .debug_selector(move || format!("annotation-ink-{name}").into())
                .child(components::form_label(name, color))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.ink.color = format!("#{color:06x}");
                    cx.notify();
                })),
            );
        }
        for width in [1, 3, 6] {
            inks = inks.child(
                components::form_button(
                    gpui::SharedString::from(format!("annotation-width-{width}")),
                    cx,
                )
                .debug_selector(move || format!("annotation-width-{width}").into())
                .child(components::form_label(
                    format!("{width} px"),
                    if self.ink.width == width as f64 {
                        theme::BLUE
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
        div()
            .id("annotation-editor")
            .size_full()
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
            .gap(px(theme::SPACE_2))
            .p(px(theme::SPACE_2))
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
            .text_color(rgb(theme::INK_STRONG))
            .child(div().child("Browser annotation"))
            .child(
                div()
                    .text_color(rgb(theme::INK_SUBTLE))
                    .child(self.retained.capture().url.clone()),
            )
            .child(tools)
            .child(inks)
            .when(self.tool == Tool::Text, |d| {
                d.child(div().child("Text label — type here, then click the image"))
                    .child(
                        div()
                            .debug_selector(|| "annotation-label".into())
                            .child(self.label.clone()),
                    )
            })
            .child(
                stage.child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            if let Some(editor) = editor {
                                let moved = editor.clone();
                                window.on_mouse_event(
                                    move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                                        if phase == gpui::DispatchPhase::Capture
                                            && event.pressed_button == Some(MouseButton::Left)
                                        {
                                            moved
                                                .update(cx, |this, cx| {
                                                    if let Some(p) = this.location(event.position) {
                                                        if let Some(points) = this.gesture.as_mut()
                                                        {
                                                            points.push(p);
                                                        }
                                                    }
                                                    cx.notify();
                                                })
                                                .ok();
                                        }
                                    },
                                );
                                window.on_mouse_event(
                                    move |event: &gpui::MouseUpEvent, phase, _, cx| {
                                        if phase == gpui::DispatchPhase::Capture
                                            && event.button == MouseButton::Left
                                        {
                                            editor
                                                .update(cx, |this, cx| {
                                                    this.finish(event.position, cx)
                                                })
                                                .ok();
                                        }
                                    },
                                );
                            }
                        },
                    )
                    .absolute()
                    .inset_0(),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(theme::SPACE_1))
                    .child(
                        components::form_button("annotation-undo", cx)
                            .child("Undo")
                            .on_click(cx.listener(|this, _, _, cx| this.undo(cx))),
                    )
                    .child(
                        components::form_button("annotation-redo", cx)
                            .child("Redo")
                            .on_click(cx.listener(|this, _, _, cx| this.redo(cx))),
                    )
                    .child(
                        components::form_button("annotation-clear", cx)
                            .child("Clear drawings")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if !this.drawings.is_empty() {
                                    this.checkpoint();
                                    this.drawings.clear();
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(div().child("Comment"))
            .child(
                div()
                    .debug_selector(|| "annotation-comment".into())
                    .child(self.comment.clone()),
            )
            .child(
                div()
                    .flex()
                    .gap(px(theme::SPACE_2))
                    .child(
                        components::primary_button("annotation-add", false, cx)
                            .debug_selector(|| "annotation-add".into())
                            .child("Add to chat")
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.emit(EditorEvent::Added {
                                    comment: this.comment.read(cx).text().to_owned(),
                                    drawings: this.drawings.clone(),
                                });
                            })),
                    )
                    .child(
                        components::form_button("annotation-cancel", cx)
                            .debug_selector(|| "annotation-cancel".into())
                            .child("Cancel")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Cancelled))),
                    ),
            )
    }
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
    let viewport = annotation.capture().viewport.clone();
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
                        paint(
                            &Drawing::Rectangle {
                                bounds: selection.clone(),
                                ink: Ink {
                                    color: format!("#{:06x}", theme::BLUE),
                                    width: 1.,
                                },
                            },
                            bounds,
                            &viewport,
                            window,
                        );
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
        Drawing::Text { .. } | Drawing::Step { .. } => return,
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
