//! The provider's control of a Browser's pages, and the live annotation's
//! capture of one, through the engine's control worker (ADR 0041, What does
//! not get a view). Each runs one at a time, in its turn: the Server asks
//! for a control (`push`), the live annotation for a capture (`capture`),
//! and each frame the root's pump has them start and finish (`pump`)
//! against the pages as they stand, with the Server and the engine handed
//! in. Only the pump writes any of it, so it is a plain struct the root's
//! `Browsers` holds, not a view.
use super::*;

#[derive(Default)]
pub(in crate::cockpit) struct Controls {
    queue: std::collections::VecDeque<ControlRequest>,
    #[cfg(feature = "cef")]
    active: Option<ActiveControl>,
    #[cfg(feature = "cef")]
    pub(in crate::cockpit) controller:
        Option<std::sync::Arc<ferrite_pane::visual::browser_control::native::Worker>>,
    #[cfg(feature = "cef")]
    capture_queue: std::collections::VecDeque<CaptureRequest>,
    #[cfg(feature = "cef")]
    running_capture: Option<RunningCapture>,
}

pub(super) struct ControlRequest {
    pub(super) request: u64,
    pub(super) thread: ThreadKey,
    pub(super) owner: Owner,
    pub(super) document: Document,
    pub(super) revision: u64,
    pub(super) control: ferrite_core::browser::Control,
    pub(super) dispatch: Option<String>,
}

#[cfg(feature = "cef")]
struct CaptureRequest {
    thread: ThreadKey,
    owner: Owner,
    document: Document,
    reply: std::sync::mpsc::SyncSender<
        Result<ferrite_pane::visual::browser_control::CapturedPage, String>,
    >,
    queued: std::time::Instant,
    point: Option<ferrite_core::browser_annotations::Point>,
}
#[cfg(feature = "cef")]
struct RunningCapture {
    request: CaptureRequest,
    native_generation: u64,
    epoch: u64,
    viewport: BrowserViewport,
    url: String,
    manual: Rc<RefCell<manual::Manual>>,
    answer: std::sync::mpsc::Receiver<ferrite_pane::visual::browser_control::native::Event>,
}

#[cfg(feature = "cef")]
struct ActiveControl {
    request: ControlRequest,
    native_generation: u64,
    epoch: u64,
    manual: Rc<RefCell<manual::Manual>>,
    answer: std::sync::mpsc::Receiver<ferrite_pane::visual::browser_control::native::Event>,
    phase: Phase,
}

#[cfg(feature = "cef")]
enum Phase {
    Attaching,
    AwaitingGrant {
        started: std::time::Instant,
        permission:
            std::sync::mpsc::SyncSender<ferrite_pane::visual::browser_control::native::Permission>,
        answer: ferrite_client::cockpit::Ask<u64>,
    },
    Running,
}

#[cfg(feature = "cef")]
impl ActiveControl {
    fn unchanged(&self, threads: &Threads) -> bool {
        control_page(threads, &self.request).is_some_and(|page| {
            page.native.state().document_generation == self.native_generation
                && page.native.control_epoch() == self.epoch
        })
    }

    fn interrupted(&self, message: &str) -> ferrite_core::browser::Outcome {
        use ferrite_core::browser::Outcome;
        if matches!(self.phase, Phase::Attaching) || self.request.control.observation() {
            Outcome::Failed {
                error: message.into(),
            }
        } else {
            Outcome::Uncertain { error: format!("{message}; observe the page before continuing and do not retry writes automatically") }
        }
    }
}

impl Controls {
    /// A control the Server asks for, run in its turn.
    pub(super) fn push(&mut self, request: ControlRequest) {
        self.queue.push_back(request);
    }

    /// Capture callers retain their Annotation lease through Server retention.
    /// Results never borrow the live View or use its last OnPaint frame.
    pub(super) fn capture(
        &mut self,
        thread: ThreadKey,
        owner: Owner,
        document: Document,
        point: Option<ferrite_core::browser_annotations::Point>,
    ) -> std::sync::mpsc::Receiver<
        Result<ferrite_pane::visual::browser_control::CapturedPage, String>,
    > {
        let (reply, answer) = std::sync::mpsc::sync_channel(1);
        #[cfg(feature = "cef")]
        self.capture_queue.push_back(CaptureRequest {
            thread,
            owner,
            document,
            reply,
            queued: std::time::Instant::now(),
            point,
        });
        #[cfg(not(feature = "cef"))]
        {
            let _ = (thread, owner, document, point);
            let _ = reply.send(Err(
                "Conversation browser capture is not available in this build".into(),
            ));
        }
        answer
    }

    /// This frame's turn: the capture under way, or the next, then the
    /// control under way, or the next, each checked against the `threads`'
    /// pages as they stand. A control's outcome goes back to the Server.
    pub(super) fn pump(
        &mut self,
        threads: &Threads,
        cockpit: &mut Cockpit,
        cx: &mut gpui::App,
    ) {
        #[cfg(feature = "cef")]
        self.pump_captures(threads, cx);
        self.pump_controls(threads, cockpit, cx);
    }

    #[cfg(feature = "cef")]
    fn pump_captures(&mut self, threads: &Threads, cx: &mut gpui::App) {
        use ferrite_core::browser::{Control, Outcome};
        use ferrite_pane::visual::browser_control::{
            native::{screenshot_bytes, Event as NativeEvent, Worker},
            CapturedPage,
        };
        if let Some(running) = self.running_capture.take() {
            let unchanged = || {
                threads
                    .get(&running.request.thread)
                    .filter(|browser| {
                        browser.owner == Some(running.request.owner)
                            && browser.state.active == Some(running.request.document.tab)
                    })
                    .and_then(|browser| browser.pages.get(&running.request.document.tab))
                    .is_some_and(|page| {
                        page.document.get() == running.request.document
                            && page.pending_navigation.borrow().is_none()
                            && !page.native.state().loading
                            && page.native.state().url == running.url
                            && page.native.state().document_generation == running.native_generation
                            && page.native.browser_viewport() == Some(running.viewport)
                            && page.native.control_epoch() == running.epoch
                    })
            };
            let result = loop {
                match running.answer.try_recv() {
                    Ok(NativeEvent::Ready(authorize)) => {
                        let permission = if unchanged() {
                            ferrite_pane::visual::browser_control::native::Permission::Grant {
                                deadline: std::time::Instant::now() + std::time::Duration::from_secs(30),
                            }
                        } else {
                            ferrite_pane::visual::browser_control::native::Permission::Deny
                        };
                        let _ = authorize.send(permission);
                    }
                    Ok(NativeEvent::Finished(Outcome::Success { value })) => {
                        break Some(if unchanged() {
                            screenshot_bytes(&value).map(|png| CapturedPage {
                                document: running.request.document,
                                native_generation: running.native_generation,
                                url: value
                                    .get("url")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or(&running.url)
                                    .to_string(),
                                viewport: ferrite_core::browser_annotations::Viewport {
                                    width_css: running.viewport.width as f64,
                                    height_css: running.viewport.height as f64,
                                    device_scale: running.viewport.scale as f64,
                                },
                                png,
                                // Without exact hit-tested DOM evidence the
                                // editor offers region feedback explicitly.
                                element: value
                                    .get("element")
                                    .cloned()
                                    .and_then(|value| serde_json::from_value(value).ok()),
                            })
                        } else {
                            Err(
                                "The page or viewport changed during capture; capture it again"
                                    .into(),
                            )
                        });
                    }
                    Ok(NativeEvent::Finished(
                        Outcome::Failed { error } | Outcome::Uncertain { error },
                    )) => break Some(Err(error)),
                    Ok(NativeEvent::Finished(Outcome::Unknown)) => {
                        break Some(Err(
                            "Browser capture completion is unknown; capture again".into()
                        ))
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        break Some(Err("Browser capture worker disconnected".into()))
                    }
                }
            };
            if let Some(result) = result {
                running.manual.borrow_mut().release_local();
                let _ = running.request.reply.send(result);
            } else {
                self.running_capture = Some(running);
                return;
            }
        }
        let Some(request) = self.capture_queue.pop_front() else {
            return;
        };
        let page = threads
            .get(&request.thread)
            .filter(|browser| {
                browser.owner == Some(request.owner)
                    && browser.state.active == Some(request.document.tab)
            })
            .and_then(|browser| browser.pages.get(&request.document.tab))
            .filter(|page| {
                page.document.get() == request.document
                    && page.pending_navigation.borrow().is_none()
                    && !page.native.state().loading
            });
        let Some(page) =
            page.filter(|_| request.queued.elapsed() < std::time::Duration::from_secs(35))
        else {
            let _ = request.reply.send(Err(
                "The selected browser document is no longer available; capture it again".into(),
            ));
            return;
        };
        let Some(viewport) = page.native.browser_viewport() else {
            let _ = request
                .reply
                .send(Err("The exact browser viewport is unavailable".into()));
            return;
        };
        let Some(target) = page.native.target_id() else {
            let _ = request
                .reply
                .send(Err("The live browser target is unavailable".into()));
            return;
        };
        let endpoint = match Visuals::engine(cx) {
            Availability::Ready(engine) => engine.controller(),
            _ => Err("The owning browser engine is unavailable".into()),
        };
        let endpoint = match endpoint {
            Ok(endpoint) => endpoint,
            Err(error) => {
                let _ = request.reply.send(Err(error));
                return;
            }
        };
        let worker = self.controller.get_or_insert_with(Worker::shared);
        let state = page.native.state();
        let epoch = page.native.control_epoch();
        page.manual.borrow_mut().protect_local();
        let answer = worker.try_start(
            endpoint,
            target,
            request.document,
            page.manual.borrow().revision,
            epoch,
            Control::Screenshot,
            request.point.clone(),
        );
        if let Some(answer) = answer {
            self.running_capture = Some(RunningCapture {
                request,
                native_generation: state.document_generation,
                epoch,
                viewport,
                url: state.url,
                manual: page.manual.clone(),
                answer,
            });
        } else {
            page.manual.borrow_mut().release_local();
            self.capture_queue.push_front(request);
        }
    }

    fn pump_controls(
        &mut self,
        threads: &Threads,
        cockpit: &mut Cockpit,
        cx: &mut gpui::App,
    ) {
        use ferrite_core::browser::Outcome;
        #[cfg(feature = "cef")]
        if let Some(mut running) = self.active.take() {
            let result = loop {
                let granted = match &mut running.phase {
                    Phase::AwaitingGrant { answer, .. } => answer.answer(),
                    _ => None,
                };
                if let Phase::AwaitingGrant {
                    started,
                    permission,
                    ..
                } = &running.phase
                {
                    use ferrite_pane::visual::browser_control::native::Permission;
                    if !running.unchanged(threads) {
                        let _ = permission.send(Permission::Deny);
                        break Some(running.interrupted("The page or operator state changed while dispatch authorization was pending"));
                    }
                    match granted {
                        Some(Ok(remaining_ms)) => {
                            let deadline =
                                started.checked_add(std::time::Duration::from_millis(remaining_ms));
                            let Some(deadline) =
                                deadline.filter(|until| *until > std::time::Instant::now())
                            else {
                                let _ = permission.send(Permission::Deny);
                                break Some(
                                    running.interrupted("Browser dispatch authorization expired"),
                                );
                            };
                            if permission.send(Permission::Grant { deadline }).is_err() {
                                break Some(running.interrupted(
                                    "Browser worker ended before receiving dispatch authorization",
                                ));
                            }
                            running.phase = Phase::Running;
                        }
                        Some(Err(error)) if error.kind() == std::io::ErrorKind::NotConnected => {
                            let _ = permission.send(Permission::Deny);
                            break Some(
                                running.interrupted("Browser dispatch authorization disconnected"),
                            );
                        }
                        Some(Err(error)) => {
                            let _ = permission.send(Permission::Deny);
                            break Some(running.interrupted(&error.to_string()));
                        }
                        None => {}
                    }
                }
                match running.answer.try_recv() {
                    Ok(ferrite_pane::visual::browser_control::native::Event::Ready(authorize)) => {
                        use ferrite_pane::visual::browser_control::native::Permission;
                        if !matches!(running.phase, Phase::Attaching)
                            || !running.unchanged(threads)
                        {
                            let _ = authorize.send(Permission::Deny);
                            break Some(running.interrupted("The owned browser document or operator revision changed before dispatch"));
                        }
                        let started = std::time::Instant::now();
                        let request = &running.request;
                        // A dispatch that could not be sent is answered so,
                        // as a grant that never came (AwaitingGrant).
                        let answer = cockpit.browser_dispatch(
                            request.request,
                            request
                                .dispatch
                                .clone()
                                .expect("dispatch capability checked before attachment"),
                            request.thread,
                            request.owner,
                            request.document,
                            request.revision,
                        );
                        running.phase = Phase::AwaitingGrant {
                            started,
                            permission: authorize,
                            answer,
                        };
                    }
                    Ok(ferrite_pane::visual::browser_control::native::Event::Finished(outcome)) => {
                        break Some(outcome)
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        break Some(running.interrupted("Browser worker disconnected"))
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break None,
                }
            };
            if let Some(mut outcome) = result {
                let current = threads
                    .get(&running.request.thread)
                    .filter(|browser| browser.owner == Some(running.request.owner))
                    .and_then(|browser| browser.pages.get(&running.request.document.tab));
                let unchanged = current.is_some_and(|page| {
                    page.document.get() == running.request.document
                        && page.native.state().document_generation == running.native_generation
                        && page.native.control_epoch() == running.epoch
                        && (!matches!(
                            running.request.control,
                            ferrite_core::browser::Control::Screenshot
                        ) || !page.native.state().loading)
                });
                if !unchanged {
                    outcome = running
                        .interrupted("The page or operator state changed during browser control");
                }
                running.manual.borrow_mut().release_local();
                reply_browser_control(cockpit, running.request, outcome);
            } else {
                self.active = Some(running);
                return;
            }
        }
        loop {
            let Some(request) = self.queue.pop_front() else {
                return;
            };
            if request.dispatch.is_none() {
                reply_browser_control(cockpit, request, Outcome::Failed {
                    error: "Update the Ferrite server to control this browser safely; it does not support dispatch authorization".into(),
                });
                continue;
            }
            let valid = control_page(threads, &request).is_some();
            if !valid {
                reply_browser_control(cockpit, request, Outcome::Failed { error: "The owned browser document or operator revision changed before dispatch; obtain fresh State and Observe".into() });
                continue;
            }
            #[cfg(not(feature = "cef"))]
            {
                let _ = cx;
                reply_browser_control(
                    cockpit,
                    request,
                    Outcome::Failed {
                        error: "Browser agent control is not available in this build".into(),
                    },
                );
            }
            #[cfg(feature = "cef")]
            {
                let endpoint = match Visuals::engine(cx) {
                    Availability::Ready(engine) => engine.controller(),
                    _ => Err("The owning browser engine is unavailable".into()),
                };
                let endpoint = match endpoint {
                    Ok(endpoint) => endpoint,
                    Err(error) => {
                        reply_browser_control(cockpit, request, Outcome::Failed { error });
                        continue;
                    }
                };
                let page = threads
                    .get(&request.thread)
                    .and_then(|browser| browser.pages.get(&request.document.tab))
                    .expect("control eligibility checked above");
                let Some(target) = page.native.target_id() else {
                    reply_browser_control(
                        cockpit,
                        request,
                        Outcome::Failed {
                            error: "The exact live browser target is not ready; observe again"
                                .into(),
                        },
                    );
                    continue;
                };
                let worker = self.controller.get_or_insert_with(
                    ferrite_pane::visual::browser_control::native::Worker::shared,
                );
                let generation = page.native.state().document_generation;
                let epoch = page.native.control_epoch();
                // This direct edge precedes dispatch and cannot disappear in a
                // coalesced Server state update.
                page.manual.borrow_mut().protect_local();
                let answer = worker.try_start(
                    endpoint,
                    target,
                    request.document,
                    request.revision,
                    epoch,
                    request.control.clone(),
                    None,
                );
                if let Some(answer) = answer {
                    self.active = Some(ActiveControl {
                        request,
                        native_generation: generation,
                        epoch,
                        manual: page.manual.clone(),
                        answer,
                        phase: Phase::Attaching,
                    });
                } else {
                    page.manual.borrow_mut().release_local();
                    self.queue.push_front(request);
                }
                return;
            }
        }
    }
}

fn reply_browser_control(
    cockpit: &mut Cockpit,
    request: ControlRequest,
    outcome: ferrite_core::browser::Outcome,
) {
    cockpit.browser_reply(
        request.request,
        request.dispatch,
        request.thread,
        request.owner,
        request.document,
        request.revision,
        outcome,
    );
}
