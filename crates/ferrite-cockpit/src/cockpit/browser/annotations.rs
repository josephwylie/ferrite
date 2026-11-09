//! Capture is protected only until the Server retains its immutable evidence.
//! The later feedback editor owns a receipt, never a live DOM reference.
use super::*;
use ferrite_core::browser::{Lease, Protection};
use ferrite_core::browser_annotations::{Annotation, Capture, Point, Rect, Selection, Viewport};
use ferrite_pane::annotation_editor::{AnnotationEditor, EditorEvent};
use ferrite_pane::visual::browser_control::CapturedPage;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

pub(super) struct Session {
    guard: Option<Guard>,
    phase: Phase,
    focus: gpui::FocusHandle,
    started: Instant,
}
struct Guard {
    owner: Owner,
    document: Document,
    generation: u64,
    lease: Lease,
    revision: u64,
    manual: Rc<RefCell<manual::Manual>>,
}
enum Phase {
    Selecting {
        element: bool,
        bounds: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
        start: Option<gpui::Point<Pixels>>,
        end: Option<gpui::Point<Pixels>>,
    },
    Capturing {
        answer: Receiver<Result<CapturedPage, String>>,
        expected: Viewport,
        region: Option<Rect>,
    },
    Encoding {
        capture: Capture,
        png: Receiver<std::io::Result<ferrite_core::protocol::WirePng>>,
    },
    Retaining(Receiver<std::io::Result<Annotation>>),
    Loading {
        retained: Annotation,
        image: Receiver<std::io::Result<std::path::PathBuf>>,
    },
    Editing(Entity<AnnotationEditor>),
}

impl CockpitView {
    pub(in crate::cockpit) fn browser_annotation_focus(
        &self,
        thread: ThreadId,
        cx: &gpui::App,
    ) -> Option<gpui::FocusHandle> {
        let session = self.browsers.threads.get(&thread)?.annotation.as_ref()?;
        Some(match &session.phase {
            Phase::Editing(editor) => gpui::Focusable::focus_handle(editor.read(cx), cx),
            _ => session.focus.clone(),
        })
    }

    pub(super) fn begin_browser_annotation(
        &mut self,
        thread: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .browsers
            .threads
            .get(&thread)
            .is_some_and(|browser| browser.annotation.is_some())
        {
            return;
        }
        let selected = self.browsers.threads.get(&thread).and_then(|browser| {
            let owner = browser.owner?;
            let tab = browser.state.active?;
            let page = browser.pages.get(&tab)?;
            Some((
                owner,
                page.document.get(),
                page.native.state().document_generation,
                page.manual.clone(),
            ))
        });
        let Some((owner, document, generation, manual)) = selected else {
            return;
        };
        self.finish_browser_interaction(thread, owner, document.tab);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        manual.borrow_mut().protect_local();
        manual.borrow_mut().capture_pause(true);
        let protected = self
            .cockpit
            .browser_input(thread, owner, document)
            .and_then(|revision| {
                manual.borrow_mut().revision = revision;
                match self
                    .cockpit
                    .browser_protect(thread, owner, document, Protection::Annotation)
                {
                    Ok(lease) => Ok((revision, lease)),
                    Err(error) => {
                        let _ = self
                            .cockpit
                            .browser_settled(thread, owner, document, revision);
                        Err(error)
                    }
                }
            });
        let browser = self.browsers.threads.get_mut(&thread).unwrap();
        match protected {
            Ok((revision, lease)) => {
                browser.error = None;
                browser.annotation = Some(Session {
                    guard: Some(Guard {
                        owner,
                        document,
                        generation,
                        lease,
                        revision,
                        manual,
                    }),
                    phase: Phase::Selecting {
                        element: false,
                        bounds: Rc::default(),
                        start: None,
                        end: None,
                    },
                    focus,
                    started: Instant::now(),
                });
            }
            Err(error) => {
                manual.borrow_mut().capture_pause(false);
                manual.borrow_mut().release_local();
                manual.borrow().restore_pause();
                browser.error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    fn release_browser_capture(&mut self, thread: ThreadId, session: &mut Session) {
        if let Some(guard) = session.guard.take() {
            let _ =
                self.cockpit
                    .browser_settled(thread, guard.owner, guard.document, guard.revision);
            let _ = self
                .cockpit
                .browser_release(thread, guard.owner, guard.lease);
            guard.manual.borrow_mut().capture_pause(false);
            guard.manual.borrow_mut().release_local();
            guard.manual.borrow().restore_pause();
        }
    }

    pub(super) fn cancel_browser_capture(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let pending = self
            .browsers
            .threads
            .get_mut(&thread)
            .and_then(|browser| browser.annotation.take());
        if let Some(mut session) = pending {
            self.release_browser_capture(thread, &mut session);
            // A retained editor survives navigation; its original page is no
            // longer needed, including when an agent navigates the browser.
            if matches!(session.phase, Phase::Editing(_) | Phase::Loading { .. }) {
                self.browsers.threads.get_mut(&thread).unwrap().annotation = Some(session);
            }
            cx.notify();
        }
    }

    fn close_browser_annotation(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if let Some(mut session) = self
            .browsers
            .threads
            .get_mut(&thread)
            .and_then(|browser| browser.annotation.take())
        {
            self.release_browser_capture(thread, &mut session);
        }
        cx.notify();
    }

    fn annotation_pointer(
        &mut self,
        thread: ThreadId,
        at: gpui::Point<Pixels>,
        down: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(Session {
            phase: Phase::Selecting { start, end, .. },
            ..
        }) = self
            .browsers
            .threads
            .get_mut(&thread)
            .and_then(|browser| browser.annotation.as_mut())
        {
            if down {
                *start = Some(at);
            }
            *end = Some(at);
            cx.notify();
        }
    }

    fn capture_browser_selection(
        &mut self,
        thread: ThreadId,
        at: gpui::Point<Pixels>,
        scale: f32,
        cx: &mut Context<Self>,
    ) {
        let request = self
            .browsers
            .threads
            .get(&thread)
            .and_then(|browser| browser.annotation.as_ref())
            .and_then(|session| {
                let guard = session.guard.as_ref()?;
                let Phase::Selecting {
                    element,
                    bounds,
                    start,
                    ..
                } = &session.phase
                else {
                    return None;
                };
                let area = bounds.get()?;
                let start = start.as_ref()?;
                let width = f32::from(area.size.width);
                let height = f32::from(area.size.height);
                if width < 1. || height < 1. {
                    return None;
                }
                // WebView rounds its native CSS viewport to whole pixels.
                // Map the overlay's fractional layout bounds by that same
                // transform rather than treating them as native CSS pixels.
                let location = |at: gpui::Point<Pixels>| Point {
                    x: f64::from(
                        f32::from(at.x - area.origin.x).clamp(0., width) / width * width.round(),
                    ),
                    y: f64::from(
                        f32::from(at.y - area.origin.y).clamp(0., height) / height * height.round(),
                    ),
                };
                let from = location(*start);
                let to = location(at);
                let region = (!element).then(|| Rect {
                    x: from.x.min(to.x),
                    y: from.y.min(to.y),
                    width: (from.x - to.x).abs(),
                    height: (from.y - to.y).abs(),
                });
                if region
                    .as_ref()
                    .is_some_and(|region| region.width < 1. || region.height < 1.)
                {
                    return None;
                }
                Some((
                    guard.owner,
                    guard.document,
                    guard.lease,
                    element.then_some(to),
                    region,
                    Viewport {
                        width_css: f64::from(width.round()),
                        height_css: f64::from(height.round()),
                        device_scale: f64::from(scale),
                    },
                ))
            });
        let Some((owner, document, lease, point, region, expected)) = request else {
            return;
        };
        let answer = self.capture_browser_page(thread, owner, document, lease, point, cx);
        if let Some(session) = self
            .browsers
            .threads
            .get_mut(&thread)
            .and_then(|browser| browser.annotation.as_mut())
        {
            session.started = Instant::now();
            session.phase = Phase::Capturing {
                answer,
                region,
                expected,
            };
        }
        cx.notify();
    }

    pub(super) fn pump_browser_annotations(&mut self, cx: &mut Context<Self>) {
        let threads: Vec<_> = self.browsers.threads.keys().copied().collect();
        for thread in threads {
            let Some(mut session) = self
                .browsers
                .threads
                .get_mut(&thread)
                .and_then(|browser| browser.annotation.take())
            else {
                continue;
            };
            let valid = matches!(session.phase, Phase::Retaining(_))
                || session.guard.as_ref().is_none_or(|guard| {
                    self.browsers.threads.get(&thread).is_some_and(|browser| {
                        browser.owner == Some(guard.owner)
                            && browser.pages.get(&guard.document.tab).is_some_and(|page| {
                                page.document.get() == guard.document
                                    && page.native.state().document_generation == guard.generation
                            })
                    })
                });
            let mut error = (!valid)
                .then(|| "The page changed during selection. Capture it again.".to_string());
            if !matches!(session.phase, Phase::Selecting { .. } | Phase::Editing(_))
                && session.started.elapsed() > Duration::from_secs(45)
            {
                error = Some("Capture timed out. Select the page again to retry.".into());
            }
            if error.is_none() {
                match &session.phase {
                    Phase::Capturing {
                        answer,
                        expected,
                        region,
                    } => match answer.try_recv() {
                        Ok(Ok(page)) => {
                            let guard = session.guard.as_ref().unwrap();
                            let aligned = page.document == guard.document
                                && page.native_generation == guard.generation
                                && (page.viewport.width_css - expected.width_css).abs() < 0.5
                                && (page.viewport.height_css - expected.height_css).abs() < 0.5
                                && (page.viewport.device_scale - expected.device_scale).abs()
                                    < 0.0001;
                            let selection = region
                                .as_ref()
                                .map(|rect| Selection::Region {
                                    x: rect.x,
                                    y: rect.y,
                                    width: rect.width,
                                    height: rect.height,
                                })
                                .or_else(|| {
                                    page.element.map(|context| Selection::Element { context })
                                });
                            if !aligned {
                                error = Some(
                                    "The page size or document changed. Select it again.".into(),
                                );
                            } else if let Some(selection) = selection {
                                let capture = Capture {
                                    document: page.document,
                                    native_generation: page.native_generation,
                                    url: page.url,
                                    viewport: page.viewport,
                                    selection,
                                };
                                let (ready, png) = std::sync::mpsc::channel();
                                match std::thread::Builder::new()
                                    .name("ferrite-capture-encode".into())
                                    .spawn(move || {
                                        let _ = ready.send(
                                            ferrite_core::browser_annotations::wire_png(&page.png),
                                        );
                                    }) {
                                    Ok(_) => session.phase = Phase::Encoding { capture, png },
                                    Err(failure) => error = Some(failure.to_string()),
                                }
                            } else {
                                error = Some("This element cannot be selected reliably. Choose Annotate, then drag a region instead.".into());
                            }
                        }
                        Ok(Err(failure)) => error = Some(failure),
                        Err(TryRecvError::Disconnected) => {
                            error = Some("The capture was interrupted. Select it again.".into())
                        }
                        Err(TryRecvError::Empty) => {}
                    },
                    Phase::Encoding { capture, png } => match png.try_recv() {
                        Ok(Ok(png)) => {
                            let guard = session.guard.as_ref().unwrap();
                            match self.cockpit.browser_capture(
                                thread,
                                guard.owner,
                                guard.lease,
                                capture.clone(),
                                png,
                            ) {
                                Ok(answer) => session.phase = Phase::Retaining(answer),
                                Err(failure) => error = Some(failure.to_string()),
                            }
                        }
                        Ok(Err(failure)) => error = Some(failure.to_string()),
                        Err(TryRecvError::Disconnected) => {
                            error =
                                Some("Capture encoding was interrupted. Select it again.".into())
                        }
                        Err(TryRecvError::Empty) => {}
                    },
                    Phase::Retaining(answer) => match answer.try_recv() {
                        Ok(Ok(retained)) => {
                            self.release_browser_capture(thread, &mut session);
                            match self
                                .cockpit
                                .browser_annotation_image(thread, retained.clone())
                            {
                                Ok(image) => session.phase = Phase::Loading { retained, image },
                                Err(failure) => error = Some(failure.to_string()),
                            }
                        }
                        Ok(Err(failure)) => error = Some(failure.to_string()),
                        Err(TryRecvError::Disconnected) => {
                            error = Some(
                                "The Server did not retain this capture. Select it again.".into(),
                            )
                        }
                        Err(TryRecvError::Empty) => {}
                    },
                    Phase::Loading { retained, image } => match image.try_recv() {
                        Ok(Ok(image)) => {
                            let receipt = retained.clone();
                            let editor =
                                cx.new(|cx| AnnotationEditor::new(receipt.clone(), image, cx));
                            cx.subscribe(&editor, move |this, _, event, cx| {
                                match event {
                                    EditorEvent::Added { comment, drawings } => {
                                        let Some(index) = this.pane_for(thread) else {
                                            return;
                                        };
                                        match receipt
                                            .with_feedback(comment.clone(), drawings.clone())
                                        {
                                            Ok(annotation) => this.panes[index].composer.update(
                                                cx,
                                                |composer, cx| {
                                                    composer.add_browser_annotation(annotation, cx)
                                                },
                                            ),
                                            Err(error) => {
                                                this.browsers
                                                    .threads
                                                    .get_mut(&thread)
                                                    .unwrap()
                                                    .error = Some(error.to_string());
                                                return;
                                            }
                                        }
                                    }
                                    EditorEvent::Cancelled => {}
                                }
                                this.close_browser_annotation(thread, cx);
                            })
                            .detach();
                            session.phase = Phase::Editing(editor);
                        }
                        Ok(Err(failure)) => error = Some(failure.to_string()),
                        Err(TryRecvError::Disconnected) => {
                            error = Some(
                                "The retained capture could not be loaded. Capture it again."
                                    .into(),
                            )
                        }
                        Err(TryRecvError::Empty) => {}
                    },
                    _ => {}
                }
            }
            if let Some(error) = error {
                self.release_browser_capture(thread, &mut session);
                self.browsers.threads.get_mut(&thread).unwrap().error = Some(error);
                cx.notify();
            } else {
                self.browsers.threads.get_mut(&thread).unwrap().annotation = Some(session);
            }
        }
    }

    pub(super) fn annotation_browser_body(
        &self,
        thread: ThreadId,
        web: Entity<WebView>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::{components::button, theme::*};
        let Some(session) = self
            .browsers
            .threads
            .get(&thread)
            .and_then(|browser| browser.annotation.as_ref())
        else {
            return div().flex_1().min_h_0().child(web).into_any_element();
        };
        if let Phase::Editing(editor) = &session.phase {
            return div()
                .flex_1()
                .min_h_0()
                .child(editor.clone())
                .into_any_element();
        }
        let mut controls = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(SPACE_2))
            .p(px(SPACE_2));
        for (label, id, element) in [
            ("Region", "browser-annotation-region", false),
            ("Select element", "browser-annotation-element", true),
        ] {
            let selected = matches!(&session.phase, Phase::Selecting { element: active, .. } if *active == element);
            controls = controls.child(
                button(id)
                    .debug_selector(move || id.into())
                    .tab_stop(true)
                    .toggled(selected)
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(Session {
                            phase:
                                Phase::Selecting {
                                    element: selected,
                                    start,
                                    end,
                                    ..
                                },
                            ..
                        }) = this
                            .browsers
                            .threads
                            .get_mut(&thread)
                            .and_then(|browser| browser.annotation.as_mut())
                        {
                            *selected = element;
                            *start = None;
                            *end = None;
                        }
                        cx.notify();
                    })),
            );
        }
        controls = controls.child(
            button("browser-annotation-cancel")
                .debug_selector(|| "browser-annotation-cancel".into())
                .tab_stop(true)
                .child("Cancel")
                .on_click(
                    cx.listener(move |this, _, _, cx| this.close_browser_annotation(thread, cx)),
                ),
        );
        let mut overlay = div()
            .id("browser-annotation-overlay")
            .debug_selector(|| "browser-annotation-overlay".into())
            .absolute()
            .inset_0()
            .occlude();
        if let Phase::Selecting {
            bounds,
            start,
            end,
            element,
        } = &session.phase
        {
            let measured = bounds.clone();
            overlay = overlay
                .child(
                    gpui::canvas(move |area, _, _| measured.set(Some(area)), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .cursor(gpui::CursorStyle::Crosshair)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.annotation_pointer(thread, event.position, true, cx);
                    }),
                )
                .on_mouse_move(
                    cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                        cx.stop_propagation();
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.annotation_pointer(thread, event.position, false, cx);
                        }
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseUpEvent, window, cx| {
                        cx.stop_propagation();
                        this.capture_browser_selection(
                            thread,
                            event.position,
                            window.scale_factor(),
                            cx,
                        );
                    }),
                );
            if !element {
                if let (Some(area), Some(start), Some(end)) = (bounds.get(), start, end) {
                    overlay = overlay.child(
                        div()
                            .absolute()
                            .left(start.x.min(end.x) - area.origin.x)
                            .top(start.y.min(end.y) - area.origin.y)
                            .w((start.x - end.x).abs())
                            .h((start.y - end.y).abs())
                            .border_1()
                            .border_color(rgb(INK_STRONG)),
                    );
                }
            }
        }
        div()
            .track_focus(&session.focus)
            .key_context("BrowserAnnotationCapture")
            .on_action(cx.listener(move |this, _: &CancelAnnotation, _, cx| {
                cx.stop_propagation();
                this.close_browser_annotation(thread, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                cx.stop_propagation();
                if event.keystroke.key == "escape" {
                    this.close_browser_annotation(thread, cx);
                }
            }))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(controls)
            .child(div().px(px(SPACE_2)).child(match &session.phase {
                Phase::Selecting { element: true, .. } => "Click an element. Escape cancels.",
                Phase::Selecting { element: false, .. } => "Drag a region. Escape cancels.",
                _ => "Retaining capture…",
            }))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(web)
                    .child(overlay),
            )
            .into_any_element()
    }
}
