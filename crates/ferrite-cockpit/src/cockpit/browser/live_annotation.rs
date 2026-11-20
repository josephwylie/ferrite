//! The live annotation of a Browser's page (ADR 0041, What does not get a
//! view): the Server's lease and pause protection it holds from Attach
//! through capture and retention, the in-page editor's events, the capture
//! through the engine's queue (`Controls`), the encode, and the annotation
//! the Server retains for the Thread's Composer. Each frame the root's pump
//! has it step (`tend`) with the Thread's Browser, the Server and the
//! capture queue handed in, and does what the step leaves to it: give the
//! annotation to the Composer, or run the action taken on the page while it
//! was open. What a press asks of it (begin, cancel, close, an action that
//! waits for the page to be restored) are calls the root makes on it.
//! A retained capture is edited in its review (`review.rs`), with no live page.
use super::*;
use ferrite_core::browser::{Lease, Protection};
use ferrite_core::browser_annotations::{
    Annotation, Capture, LiveFeedback, LiveScreenshot, Selection, Viewport,
};
use ferrite_pane::visual::browser_control::CapturedPage;
use ferrite_pane::visual::live_annotation::{self, Event as LiveEvent, Submission};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

pub(super) struct LiveAnnotation {
    guard: Option<Guard>,
    phase: Phase,
    focus: gpui::FocusHandle,
    started: Instant,
    window: Option<gpui::AnyWindowHandle>,
    submission: Submission,
    action: Option<Action>,
    restore_failed: bool,
    recovering: bool,
    /// The guest editor has installed its listeners and takes page input.
    ready: bool,
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
}

/// The page an annotation begins on: the Browser's shown page, settled.
pub(super) struct Target {
    pub(super) owner: Owner,
    pub(super) document: Document,
    generation: u64,
    manual: Rc<RefCell<manual::Manual>>,
    native: Rc<dyn BrowserView>,
    pub(super) focus: gpui::FocusHandle,
}

/// What a frame's step leaves the root to do.
pub(super) struct Step {
    /// Why the annotation closes, for the Browser's notice.
    pub(super) error: Option<String>,
    /// The window draws again.
    pub(super) changed: bool,
    pub(super) next: Next,
}

pub(super) enum Next {
    /// Still open.
    Open(LiveAnnotation),
    /// The Server retained it: it goes to the Thread's Composer, which sends
    /// it in the window given.
    Attach(Annotation, Option<gpui::AnyWindowHandle>),
    /// Closed, with the action taken on its page while it was open, to run
    /// now.
    Closed(Option<Action>),
}

impl LiveAnnotation {
    /// The page `browser` shows, if one can be annotated: live, drawn and
    /// loaded.
    pub(super) fn target(browser: &Browser, cx: &gpui::App) -> Option<Target> {
        let owner = browser.owner?;
        let page = browser.pages.get(&browser.state.active?)?;
        let state = page.native.state();
        if !page.web.read(cx).browser_visible() || state.loading {
            return None;
        }
        Some(Target {
            owner,
            document: page.document.get(),
            generation: state.document_generation,
            manual: page.manual.clone(),
            native: page.native.clone(),
            focus: gpui::Focusable::focus_handle(page.web.read(cx), cx),
        })
    }

    /// Protect `target` for an annotation and start the in-page editor, its
    /// Send shown as `send_enabled` says; or why it could not begin.
    pub(super) fn begin(
        thread: ThreadId,
        target: Target,
        window: gpui::AnyWindowHandle,
        send_enabled: bool,
        cockpit: &mut Cockpit,
    ) -> Result<Self, String> {
        let Target {
            owner,
            document,
            generation,
            manual,
            native,
            focus,
        } = target;
        manual.borrow_mut().protect_local();
        manual.borrow_mut().capture_pause(true);
        let protected = cockpit
            .browser_input(thread, owner, document)
            .and_then(|revision| {
                manual.borrow_mut().revision = revision;
                match cockpit.browser_protect(thread, owner, document, Protection::Annotation) {
                    Ok(lease) => Ok((revision, lease)),
                    Err(error) => {
                        let _ = cockpit.browser_settled(thread, owner, document, revision);
                        Err(error)
                    }
                }
            });
        match protected {
            Ok((revision, lease)) => {
                let mut annotation = Self {
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
                    window: Some(window),
                    submission: Submission::Attach,
                    action: None,
                    restore_failed: false,
                    recovering: false,
                    ready: false,
                };
                if let Err(error) = native.start_annotation(generation, send_enabled) {
                    annotation.guard.as_mut().unwrap().restored = true;
                    annotation.release(thread, cockpit);
                    Err(error)
                } else {
                    Ok(annotation)
                }
            }
            Err(error) => {
                manual.borrow_mut().capture_pause(false);
                manual.borrow_mut().release_local();
                manual.borrow().restore_pause();
                Err(error.to_string())
            }
        }
    }

    pub(super) fn focus(&self) -> &gpui::FocusHandle {
        &self.focus
    }

    /// The in-page editor takes page input.
    #[cfg(feature = "browser-smoke")]
    pub(super) fn ready(&self) -> bool {
        self.ready && matches!(self.phase, Phase::Live)
    }

    /// `action` on the pages waits for the annotation to close, which it
    /// begins. After a failed restore only a Reload or Close of its page
    /// runs, to recover it. False when it holds no page.
    pub(super) fn defer(&mut self, action: Action) -> bool {
        let Some(guard) = &self.guard else {
            return false;
        };
        if self.restore_failed {
            let recovery = match &action {
                Action::Reload { document } => *document == guard.document,
                Action::Close { tab } => *tab == guard.document.tab,
                _ => false,
            };
            if recovery && !self.recovering {
                if let Some(native) = &guard.manual.borrow().native {
                    if native.state().document_generation == guard.generation
                        && native.reload().is_ok()
                    {
                        self.recovering = true;
                        self.action = matches!(action, Action::Close { .. }).then_some(action);
                    }
                }
            }
        } else {
            if self.action.is_none() {
                self.action = Some(action);
            }
            self.finish_guest();
        }
        self.phase = Phase::Closing;
        true
    }

    /// Stop it: one that holds its page closes as the page is restored.
    pub(super) fn cancel(&mut self) {
        if self.guard.is_some() {
            self.finish_guest();
            self.phase = Phase::Closing;
        }
    }

    /// Close it: what still holds its page closes as the page is restored;
    /// what holds none is gone now.
    pub(super) fn close(mut self) -> Option<Self> {
        if self.guard.is_none() {
            return None;
        }
        self.cancel();
        Some(self)
    }

    /// This frame's step against `browser`, the Thread's Browser as it
    /// stands. `send_enabled` is whether the Thread's Composer could send
    /// now, which the in-page editor's Send follows; `composer` whether the
    /// Thread has one to take the annotation.
    pub(super) fn tend(
        mut self,
        thread: ThreadId,
        browser: &Browser,
        cockpit: &mut Cockpit,
        controls: &mut Controls,
        send_enabled: bool,
        composer: bool,
    ) -> Step {
        let valid = self.guard.as_ref().is_none_or(|guard| {
            browser.owner == Some(guard.owner)
                && browser.pages.get(&guard.document.tab).is_some_and(|page| {
                    page.document.get() == guard.document
                        && page.native.state().document_generation == guard.generation
                })
        });
        let mut error = None;
        let mut changed = false;
        if !valid && !matches!(self.phase, Phase::Retaining(_)) {
            if !self.recovering {
                error = Some("The page changed during annotation. Select it again.".to_string());
            }
            if let Some(guard) = self.guard.as_mut() {
                guard.restored |= guard.manual.borrow().native.as_ref().is_none_or(|native| {
                    native.state().document_generation != guard.generation
                });
            }
        }
        let native = self
            .guard
            .as_ref()
            .and_then(|guard| guard.manual.borrow().native.clone());
        if let Some(native) = native {
            native.set_annotation_send_enabled(send_enabled);
            for _ in 0..4 {
                let Some(event) = native.take_annotation_event() else {
                    break;
                };
                match event {
                    LiveEvent::Ready => {
                        self.ready = true;
                        changed = true;
                    }
                    LiveEvent::Closed => {
                        if let Some(guard) = self.guard.as_mut() {
                            guard.restored = true;
                        }
                    }
                    LiveEvent::Cancelled => {
                        self.finish_guest();
                        self.phase = Phase::Closing;
                    }
                    LiveEvent::Failed(message) => error = Some(message),
                    LiveEvent::RestoreFailed(message) => {
                        self.restore_failed = true;
                        error = Some(message);
                    }
                    LiveEvent::Attach {
                        feedback,
                        viewport,
                        submission,
                    } if matches!(self.phase, Phase::Live) && valid => {
                        let guard = self.guard.as_ref().unwrap();
                        let aligned = native.browser_viewport().is_some_and(|actual| {
                            (f64::from(actual.width) - viewport.width_css).abs() < 0.5
                                && (f64::from(actual.height) - viewport.height_css).abs() < 0.5
                                && (f64::from(actual.scale) - viewport.device_scale).abs()
                                    < 0.0001
                        });
                        if aligned {
                            self.submission = submission;
                            let answer =
                                controls.capture(thread, guard.owner, guard.document, None);
                            self.phase = Phase::Capturing {
                                answer,
                                expected: viewport,
                                feedback,
                            };
                            self.started = Instant::now();
                        } else {
                            error = Some("The page size changed. Select it again.".into());
                        }
                    }
                    LiveEvent::Attach { .. } => {}
                }
            }
        }
        if !matches!(
            self.phase,
            Phase::Live | Phase::Closing | Phase::PendingAttachment(_)
        ) && self.started.elapsed() > Duration::from_secs(45)
        {
            error = Some("The annotation capture timed out. Select it again.".into());
        }
        let mut attach = None;
        if error.is_none() {
            match &mut self.phase {
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
                        let guard = self.guard.as_ref().unwrap();
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
                                    self.phase = Phase::Encoding {
                                        answer,
                                        ready: None,
                                    };
                                    self.finish_guest();
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
                    if self.guard.as_ref().is_some_and(|guard| guard.restored) {
                        if let Some((capture, png)) = ready.take() {
                            let guard = self.guard.as_ref().unwrap();
                            match cockpit.browser_capture(
                                thread,
                                guard.owner,
                                guard.lease,
                                capture,
                                png,
                            ) {
                                Ok(answer) => self.phase = Phase::Retaining(answer),
                                Err(failure) => error = Some(failure.to_string()),
                            }
                        }
                    }
                }
                Phase::Retaining(answer) => match answer.try_recv() {
                    Ok(Ok(retained)) => {
                        self.release(thread, cockpit);
                        if composer {
                            let send = (self.submission == Submission::Send)
                                .then_some(self.window)
                                .flatten();
                            attach = Some(Next::Attach(retained, send));
                        } else {
                            self.submission = Submission::Attach;
                            self.phase = Phase::PendingAttachment(retained);
                        }
                        changed = true;
                    }
                    Ok(Err(failure)) => error = Some(failure.to_string()),
                    Err(TryRecvError::Disconnected) => {
                        error = Some("The Server did not retain this annotation.".into())
                    }
                    Err(TryRecvError::Empty) => {}
                },
                Phase::PendingAttachment(retained) => {
                    if composer {
                        attach = Some(Next::Attach(retained.clone(), None));
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        if let Some(next) = attach {
            return Step {
                error,
                changed,
                next,
            };
        }
        if error.is_some() {
            self.finish_guest();
            self.phase = Phase::Closing;
            changed = true;
        }
        if matches!(self.phase, Phase::Closing)
            && self.guard.as_ref().is_none_or(|guard| guard.restored)
        {
            let action = self.action.take().filter(|action| self.guard.as_ref().is_some_and(|guard| {
                browser.owner == Some(guard.owner)
                    && browser.pages.get(&guard.document.tab).is_some_and(|page|
                        page.document.get() == guard.document || (self.recovering && matches!(action, Action::Close { tab } if *tab == guard.document.tab)))
            }));
            self.release(thread, cockpit);
            return Step {
                error,
                changed: true,
                next: Next::Closed(action),
            };
        }
        Step {
            error,
            changed,
            next: Next::Open(self),
        }
    }

    /// Give the page back: settle the operator's input, release the
    /// Server's lease and lift the pause.
    fn release(&mut self, thread: ThreadId, cockpit: &mut Cockpit) {
        if let Some(guard) = self.guard.take() {
            let _ = cockpit.browser_settled(thread, guard.owner, guard.document, guard.revision);
            let _ = cockpit.browser_release(thread, guard.owner, guard.lease);
            guard.manual.borrow_mut().capture_pause(false);
            guard.manual.borrow_mut().release_local();
            guard.manual.borrow().restore_pause();
        }
    }

    /// Ask the in-page editor to finish, which restores the page.
    fn finish_guest(&mut self) {
        if let Some(guard) = &self.guard {
            if let Some(native) = &guard.manual.borrow().native {
                native.finish_annotation(guard.generation);
            }
        }
    }
}
