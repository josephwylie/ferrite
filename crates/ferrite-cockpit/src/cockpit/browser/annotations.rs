//! A live guest session holds protection through Attach, capture, and retention.
//! Historical captures remain editable without retaining a live DOM reference.
use super::*;
use ferrite_core::browser::{Lease, Protection};
use ferrite_core::browser_annotations::{
    Annotation, Capture, LiveFeedback, LiveScreenshot, Selection, Viewport,
};
use ferrite_pane::annotation_editor::{AnnotationEditor, EditorEvent};
use ferrite_pane::visual::browser_control::CapturedPage;
use ferrite_pane::visual::live_annotation::{self, Event as LiveEvent, Submission};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

pub(super) struct Session {
    guard: Option<Guard>,
    phase: Phase,
    focus: gpui::FocusHandle,
    started: Instant,
    window: Option<gpui::AnyWindowHandle>,
    submission: Submission,
    action: Option<Action>,
    restore_failed: bool,
    recovering: bool,
}
struct Guard {
    owner: Owner,
    document: Document,
    generation: u64,
    lease: Lease,
    revision: u64,
    manual: Rc<RefCell<manual::Manual>>,
    restored: bool,
}
type EncodedCapture = (Capture, Option<ferrite_core::protocol::WirePng>);
enum Phase {
    Live,
    Capturing {
        answer: Receiver<Result<CapturedPage, String>>,
        expected: Viewport,
        feedback: LiveFeedback,
    },
    Encoding {
        answer: Receiver<std::io::Result<EncodedCapture>>,
        ready: Option<EncodedCapture>,
    },
    Retaining(Receiver<std::io::Result<Annotation>>),
    PendingAttachment(Annotation),
    Closing,
    Loading {
        retained: Annotation,
        image: Receiver<std::io::Result<std::path::PathBuf>>,
    },
    Editing(Entity<AnnotationEditor>),
}

impl CockpitView {
    #[cfg(feature = "visual-reference")]
    pub(in crate::cockpit) fn fixture_annotation_editor(
        &mut self,
        thread: ThreadId,
        annotation: Annotation,
        image: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.ensure_browser(thread, cx);
        let editor = cx.new(|cx| AnnotationEditor::new(annotation, image, cx));
        self.browsers.threads.get_mut(&thread).unwrap().annotation = Some(Session {
            guard: None,
            phase: Phase::Editing(editor),
            focus: cx.focus_handle(),
            started: Instant::now(),
            window: None,
            submission: Submission::Attach,
            action: None,
            restore_failed: false,
            recovering: false,
        });
        self.upsert_surface(thread, super::super::surfaces::SurfaceId::Browser, cx);
    }

    #[cfg(feature = "browser-smoke")]
    pub(in crate::cockpit) fn browser_smoke_viewport(
        &self,
        thread: ThreadId,
    ) -> Option<BrowserViewport> {
        let browser = self.browsers.threads.get(&thread)?;
        browser
            .pages
            .get(&browser.state.active?)?
            .native
            .browser_viewport()
    }
    #[cfg(feature = "browser-smoke")]
    pub(in crate::cockpit) fn browser_smoke_annotation_picture(
        &self,
        thread: ThreadId,
        cx: &gpui::App,
    ) -> Option<gpui::Bounds<Pixels>> {
        let session = self.browsers.threads.get(&thread)?.annotation.as_ref()?;
        match &session.phase {
            Phase::Editing(editor) => editor.read(cx).picture_bounds(),
            _ => None,
        }
    }

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
        let selected = self.browsers.threads.get(&thread).and_then(|browser| {
            if browser.annotation.is_some() || browser.entry == Entry::NewTab {
                return None;
            }
            let owner = browser.owner?;
            let page = browser.pages.get(&browser.state.active?)?;
            let state = page.native.state();
            if !page.web.read(cx).browser_visible() || state.loading {
                return None;
            }
            Some((
                owner,
                page.document.get(),
                state.document_generation,
                page.manual.clone(),
                page.native.clone(),
                gpui::Focusable::focus_handle(page.web.read(cx), cx),
            ))
        });
        let Some((owner, document, generation, manual, native, focus)) = selected else {
            return;
        };
        self.finish_browser_interaction(thread, owner, document.tab);
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
        match protected {
            Ok((revision, lease)) => {
                let mut session = Session {
                    guard: Some(Guard {
                        owner,
                        document,
                        generation,
                        lease,
                        revision,
                        manual,
                        restored: false,
                    }),
                    phase: Phase::Live,
                    focus,
                    started: Instant::now(),
                    window: Some(window.window_handle()),
                    submission: Submission::Attach,
                    action: None,
                    restore_failed: false,
                    recovering: false,
                };
                if let Err(error) =
                    native.start_annotation(generation, self.annotation_send_enabled(thread, cx))
                {
                    session.guard.as_mut().unwrap().restored = true;
                    self.release_browser_capture(thread, &mut session);
                    self.browsers.threads.get_mut(&thread).unwrap().error = Some(error);
                } else {
                    let browser = self.browsers.threads.get_mut(&thread).unwrap();
                    browser.error = None;
                    browser.annotation = Some(session);
                }
            }
            Err(error) => {
                manual.borrow_mut().capture_pause(false);
                manual.borrow_mut().release_local();
                manual.borrow().restore_pause();
                self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    fn annotation_send_enabled(&self, thread: ThreadId, cx: &gpui::App) -> bool {
        self.focused_thread() == Some(thread)
            && self.cockpit.thread(thread).is_some()
            && self.pane_for(thread).is_some_and(|index| {
                self.panes[index].is_main() && self.panes[index].composer.read(cx).comment_valid(cx)
            })
            && !self.project_editor_open(cx)
            && self.snooze_editor.is_none()
            && self.rename.is_none()
    }

    fn send_attached_annotation(
        &mut self,
        thread: ThreadId,
        id: String,
        window: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = view.update(cx, |this, cx| {
                    if this.annotation_send_enabled(thread, cx)
                        && this.pane_for(thread).is_some_and(|index| {
                            this.panes[index]
                                .composer
                                .read(cx)
                                .browser_annotations()
                                .iter()
                                .any(|annotation| annotation.id() == id)
                        })
                    {
                        this.submit(&Submit, window, cx);
                    }
                });
            });
        });
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

    fn finish_annotation_guest(session: &mut Session) {
        if let Some(guard) = &session.guard {
            if let Some(native) = &guard.manual.borrow().native {
                native.finish_annotation(guard.generation);
            }
        }
    }

    pub(super) fn defer_annotation_action(
        &mut self,
        thread: ThreadId,
        action: Action,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self
            .browsers
            .threads
            .get_mut(&thread)
            .and_then(|browser| browser.annotation.as_mut())
        else {
            return false;
        };
        let Some(guard) = &session.guard else {
            return false;
        };
        if session.restore_failed {
            let recovery = match &action {
                Action::Reload { document } => *document == guard.document,
                Action::Close { tab } => *tab == guard.document.tab,
                _ => false,
            };
            if recovery && !session.recovering {
                if let Some(native) = &guard.manual.borrow().native {
                    if native.state().document_generation == guard.generation
                        && native.reload().is_ok()
                    {
                        session.recovering = true;
                        session.action = matches!(action, Action::Close { .. }).then_some(action);
                    }
                }
            }
        } else {
            if session.action.is_none() {
                session.action = Some(action);
            }
            Self::finish_annotation_guest(session);
        }
        session.phase = Phase::Closing;
        cx.notify();
        true
    }

    pub(super) fn cancel_browser_capture(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(session) = self
            .browsers
            .threads
            .get_mut(&thread)
            .and_then(|browser| browser.annotation.as_mut())
        else {
            return;
        };
        if session.guard.is_some() {
            Self::finish_annotation_guest(session);
            session.phase = Phase::Closing;
        }
        cx.notify();
    }

    pub(super) fn close_browser_annotation(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(mut session) = self
            .browsers
            .threads
            .get_mut(&thread)
            .and_then(|browser| browser.annotation.take())
        else {
            return;
        };
        if session.guard.is_some() {
            Self::finish_annotation_guest(&mut session);
            session.phase = Phase::Closing;
            self.browsers.threads.get_mut(&thread).unwrap().annotation = Some(session);
        }
        cx.notify();
    }

    pub(super) fn pump_browser_annotations(&mut self, cx: &mut Context<Self>) {
        let edits: Vec<_> = self
            .panes
            .iter()
            .flat_map(|pane| {
                let thread = pane.thread();
                pane.preview
                    .take_annotation_edits()
                    .into_iter()
                    .filter_map(move |annotation| thread.map(|thread| (thread, annotation)))
            })
            .collect();
        for (thread, retained) in edits {
            self.ensure_browser(thread, cx);
            if self.browsers.threads[&thread].annotation.is_some() {
                self.browsers.threads.get_mut(&thread).unwrap().error =
                    Some("Finish or cancel the current annotation before opening another.".into());
                continue;
            }
            if retained.original().is_none() {
                self.browsers.threads.get_mut(&thread).unwrap().error = Some("This annotation has no screenshot. Its comment and targets are retained in the attachment.".into());
                continue;
            }
            match self
                .cockpit
                .browser_annotation_image(thread, retained.clone())
            {
                Ok(image) => {
                    self.browsers.threads.get_mut(&thread).unwrap().annotation = Some(Session {
                        guard: None,
                        phase: Phase::Loading { retained, image },
                        focus: cx.focus_handle(),
                        started: Instant::now(),
                        window: None,
                        submission: Submission::Attach,
                        action: None,
                        restore_failed: false,
                        recovering: false,
                    })
                }
                Err(error) => {
                    self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string())
                }
            }
            cx.notify();
        }
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
            let valid = session.guard.as_ref().is_none_or(|guard| {
                self.browsers.threads.get(&thread).is_some_and(|browser| {
                    browser.owner == Some(guard.owner)
                        && browser.pages.get(&guard.document.tab).is_some_and(|page| {
                            page.document.get() == guard.document
                                && page.native.state().document_generation == guard.generation
                        })
                })
            });
            let mut error = None;
            let mut handed_off = false;
            if !valid && !matches!(session.phase, Phase::Retaining(_)) {
                if !session.recovering {
                    error =
                        Some("The page changed during annotation. Select it again.".to_string());
                }
                if let Some(guard) = session.guard.as_mut() {
                    guard.restored |= guard.manual.borrow().native.as_ref().is_none_or(|native| {
                        native.state().document_generation != guard.generation
                    });
                }
            }
            let native = session
                .guard
                .as_ref()
                .and_then(|guard| guard.manual.borrow().native.clone());
            if let Some(native) = native {
                native.set_annotation_send_enabled(self.annotation_send_enabled(thread, cx));
                for _ in 0..4 {
                    let Some(event) = native.take_annotation_event() else {
                        break;
                    };
                    match event {
                        LiveEvent::Ready => cx.notify(),
                        LiveEvent::Closed => {
                            if let Some(guard) = session.guard.as_mut() {
                                guard.restored = true;
                            }
                        }
                        LiveEvent::Cancelled => {
                            Self::finish_annotation_guest(&mut session);
                            session.phase = Phase::Closing;
                        }
                        LiveEvent::Failed(message) => error = Some(message),
                        LiveEvent::RestoreFailed(message) => {
                            session.restore_failed = true;
                            error = Some(message);
                        }
                        LiveEvent::Attach {
                            feedback,
                            viewport,
                            submission,
                        } if matches!(session.phase, Phase::Live) && valid => {
                            let guard = session.guard.as_ref().unwrap();
                            let aligned = native.browser_viewport().is_some_and(|actual| {
                                (f64::from(actual.width) - viewport.width_css).abs() < 0.5
                                    && (f64::from(actual.height) - viewport.height_css).abs() < 0.5
                                    && (f64::from(actual.scale) - viewport.device_scale).abs()
                                        < 0.0001
                            });
                            if aligned {
                                session.submission = submission;
                                let answer = self.capture_browser_page(
                                    thread,
                                    guard.owner,
                                    guard.document,
                                    guard.lease,
                                    None,
                                    cx,
                                );
                                session.phase = Phase::Capturing {
                                    answer,
                                    expected: viewport,
                                    feedback,
                                };
                                session.started = Instant::now();
                            } else {
                                error = Some("The page size changed. Select it again.".into());
                            }
                        }
                        LiveEvent::Attach { .. } => {}
                    }
                }
            }
            if !matches!(
                session.phase,
                Phase::Live | Phase::Editing(_) | Phase::Closing | Phase::PendingAttachment(_)
            ) && session.started.elapsed() > Duration::from_secs(45)
            {
                error = Some("The annotation capture timed out. Select it again.".into());
            }
            if error.is_none() {
                match &mut session.phase {
                    Phase::Capturing {
                        answer,
                        expected,
                        feedback,
                    } => {
                        let result = match answer.try_recv() {
                            Ok(result) => Some(result),
                            Err(TryRecvError::Disconnected) => {
                                Some(Err("The screenshot capture was interrupted".into()))
                            }
                            Err(TryRecvError::Empty) => None,
                        };
                        if let Some(result) = result {
                            let guard = session.guard.as_ref().unwrap();
                            let mut capture = Capture {
                                document: guard.document,
                                native_generation: guard.generation,
                                url: feedback.page_url.clone(),
                                viewport: expected.clone(),
                                selection: Selection::Live {
                                    feedback: feedback.clone(),
                                },
                            };
                            let aligned = result.as_ref().is_ok_and(|page| {
                                page.document == guard.document
                                    && page.native_generation == guard.generation
                                    && page.url == capture.url
                                    && (page.viewport.width_css - expected.width_css).abs() < 0.5
                                    && (page.viewport.height_css - expected.height_css).abs() < 0.5
                                    && (page.viewport.device_scale - expected.device_scale).abs()
                                        < 0.0001
                            });
                            if result.is_ok() && !aligned {
                                error = Some(
                                    "The captured document or page size changed. Select it again."
                                        .into(),
                                );
                            } else {
                                let (ready, answer) = std::sync::mpsc::channel();
                                let spawned = std::thread::Builder::new()
                                    .name("ferrite-live-annotation-encode".into())
                                    .spawn(move || {
                                        let Selection::Live { feedback } = &mut capture.selection
                                        else {
                                            unreachable!()
                                        };
                                        let cropped = result.and_then(|page| {
                                            live_annotation::crop(
                                                &page.png,
                                                &capture.viewport,
                                                &feedback.bounds,
                                            )
                                        });
                                        let encoded = match cropped {
                                            Ok((png, crop)) => {
                                                feedback.screenshot =
                                                    LiveScreenshot::Captured { crop };
                                                ferrite_core::browser_annotations::wire_png(&png)
                                                    .map(Some)
                                            }
                                            Err(reason) => {
                                                feedback.screenshot =
                                                    LiveScreenshot::Unavailable { reason };
                                                Ok(None)
                                            }
                                        }
                                        .map(|png| (capture, png));
                                        let _ = ready.send(encoded);
                                    });
                                match spawned {
                                    Ok(_) => {
                                        session.phase = Phase::Encoding {
                                            answer,
                                            ready: None,
                                        };
                                        Self::finish_annotation_guest(&mut session);
                                    }
                                    Err(failure) => error = Some(failure.to_string()),
                                }
                            }
                        }
                    }
                    Phase::Encoding { answer, ready } => {
                        if ready.is_none() {
                            match answer.try_recv() {
                                Ok(Ok(encoded)) => *ready = Some(encoded),
                                Ok(Err(failure)) => error = Some(failure.to_string()),
                                Err(TryRecvError::Disconnected) => {
                                    error = Some("The annotation encoding was interrupted.".into())
                                }
                                Err(TryRecvError::Empty) => {}
                            }
                        }
                        if session.guard.as_ref().is_some_and(|guard| guard.restored) {
                            if let Some((capture, png)) = ready.take() {
                                let guard = session.guard.as_ref().unwrap();
                                match self.cockpit.browser_capture(
                                    thread,
                                    guard.owner,
                                    guard.lease,
                                    capture,
                                    png,
                                ) {
                                    Ok(answer) => session.phase = Phase::Retaining(answer),
                                    Err(failure) => error = Some(failure.to_string()),
                                }
                            }
                        }
                    }
                    Phase::Retaining(answer) => match answer.try_recv() {
                        Ok(Ok(retained)) => {
                            self.release_browser_capture(thread, &mut session);
                            if let Some(index) = self.pane_for(thread) {
                                let id = retained.id().to_owned();
                                self.panes[index].composer.update(cx, |composer, cx| {
                                    composer.add_browser_annotation(retained, cx)
                                });
                                if session.submission == Submission::Send {
                                    if let Some(window) = session.window {
                                        self.send_attached_annotation(thread, id, window, cx);
                                    }
                                }
                                handed_off = true;
                            } else {
                                session.submission = Submission::Attach;
                                session.phase = Phase::PendingAttachment(retained);
                            }
                            cx.notify();
                        }
                        Ok(Err(failure)) => error = Some(failure.to_string()),
                        Err(TryRecvError::Disconnected) => {
                            error = Some("The Server did not retain this annotation.".into())
                        }
                        Err(TryRecvError::Empty) => {}
                    },
                    Phase::PendingAttachment(retained) => {
                        if let Some(index) = self.pane_for(thread) {
                            let annotation = retained.clone();
                            self.panes[index].composer.update(cx, |composer, cx| {
                                composer.add_browser_annotation(annotation, cx)
                            });
                            handed_off = true;
                            cx.notify();
                        }
                    }
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
                            cx.notify();
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
                self.browsers.threads.get_mut(&thread).unwrap().error = Some(error);
                Self::finish_annotation_guest(&mut session);
                session.phase = Phase::Closing;
                cx.notify();
            }
            if matches!(session.phase, Phase::Closing)
                && session.guard.as_ref().is_none_or(|guard| guard.restored)
            {
                let action = session.action.take().filter(|action| session.guard.as_ref().is_some_and(|guard| {
                    self.browsers.threads.get(&thread).is_some_and(|browser| browser.owner == Some(guard.owner)
                        && browser.pages.get(&guard.document.tab).is_some_and(|page|
                            page.document.get() == guard.document || (session.recovering && matches!(action, Action::Close { tab } if *tab == guard.document.tab))))
                }));
                self.release_browser_capture(thread, &mut session);
                handed_off = true;
                if let Some(action) = action {
                    self.browser_action(thread, action, cx);
                }
                cx.notify();
            }
            if !handed_off {
                self.browsers.threads.get_mut(&thread).unwrap().annotation = Some(session);
            }
        }
    }

    pub(super) fn annotation_browser_body(
        &self,
        thread: ThreadId,
        web: Option<Entity<WebView>>,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(Session {
            phase: Phase::Editing(editor),
            ..
        }) = self
            .browsers
            .threads
            .get(&thread)
            .and_then(|browser| browser.annotation.as_ref())
        {
            div()
                .flex_1()
                .min_h_0()
                .child(editor.clone())
                .into_any_element()
        } else {
            div().flex_1().min_h_0().children(web).into_any_element()
        }
    }
}
