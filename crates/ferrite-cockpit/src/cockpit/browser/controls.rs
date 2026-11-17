use super::*;

#[derive(Default)]
pub(super) struct PendingControls {
    queue: std::collections::VecDeque<ControlRequest>,
    #[cfg(feature = "cef")]
    active: Option<ActiveControl>,
}

impl PendingControls {
    pub(super) fn push(&mut self, request: ControlRequest) {
        self.queue.push_back(request);
    }
}

pub(super) struct ControlRequest {
    pub(super) request: u64,
    pub(super) thread: ThreadId,
    pub(super) owner: Owner,
    pub(super) document: Document,
    pub(super) revision: u64,
    pub(super) control: ferrite_core::browser::Control,
    pub(super) dispatch: Option<String>,
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
        answer: std::sync::mpsc::Receiver<std::io::Result<u64>>,
    },
    Running,
}

#[cfg(feature = "cef")]
impl ActiveControl {
    fn unchanged(&self, browsers: &Browsers) -> bool {
        browsers.control_page(&self.request).is_some_and(|page| {
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

impl CockpitView {
    fn reply_browser_control(
        &mut self,
        request: ControlRequest,
        outcome: ferrite_core::browser::Outcome,
    ) {
        self.cockpit.browser_reply(
            request.request,
            request.dispatch,
            request.thread,
            request.owner,
            request.document,
            request.revision,
            outcome,
        );
    }

    pub(super) fn pump_browser_controls(&mut self, cx: &mut Context<Self>) {
        use ferrite_core::browser::Outcome;
        #[cfg(feature = "cef")]
        if let Some(mut running) = self.browsers.controls.active.take() {
            let result = loop {
                if let Phase::AwaitingGrant {
                    started,
                    permission,
                    answer,
                } = &running.phase
                {
                    use ferrite_pane::visual::browser_control::native::Permission;
                    if !running.unchanged(&self.browsers) {
                        let _ = permission.send(Permission::Deny);
                        break Some(running.interrupted("The page or operator state changed while dispatch authorization was pending"));
                    }
                    match answer.try_recv() {
                        Ok(Ok(remaining_ms)) => {
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
                        Ok(Err(error)) => {
                            let _ = permission.send(Permission::Deny);
                            break Some(running.interrupted(&error.to_string()));
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            let _ = permission.send(Permission::Deny);
                            break Some(
                                running.interrupted("Browser dispatch authorization disconnected"),
                            );
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    }
                }
                match running.answer.try_recv() {
                    Ok(ferrite_pane::visual::browser_control::native::Event::Ready(authorize)) => {
                        use ferrite_pane::visual::browser_control::native::Permission;
                        if !matches!(running.phase, Phase::Attaching)
                            || !running.unchanged(&self.browsers)
                        {
                            let _ = authorize.send(Permission::Deny);
                            break Some(running.interrupted("The owned browser document or operator revision changed before dispatch"));
                        }
                        let started = std::time::Instant::now();
                        let request = &running.request;
                        match self.cockpit.browser_dispatch(
                            request.request,
                            request
                                .dispatch
                                .clone()
                                .expect("dispatch capability checked before attachment"),
                            request.thread,
                            request.owner,
                            request.document,
                            request.revision,
                        ) {
                            Ok(answer) => {
                                running.phase = Phase::AwaitingGrant {
                                    started,
                                    permission: authorize,
                                    answer,
                                };
                            }
                            Err(error) => {
                                let _ = authorize.send(Permission::Deny);
                                break Some(Outcome::Failed {
                                    error: error.to_string(),
                                });
                            }
                        }
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
                let current = self
                    .browsers
                    .threads
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
                self.reply_browser_control(running.request, outcome);
            } else {
                self.browsers.controls.active = Some(running);
                return;
            }
        }
        loop {
            let Some(request) = self.browsers.controls.queue.pop_front() else {
                return;
            };
            if request.dispatch.is_none() {
                self.reply_browser_control(request, Outcome::Failed {
                    error: "Update the Ferrite server to control this browser safely; it does not support dispatch authorization".into(),
                });
                continue;
            }
            let valid = self.browsers.control_page(&request).is_some();
            if !valid {
                self.reply_browser_control(request, Outcome::Failed { error: "The owned browser document or operator revision changed before dispatch; obtain fresh State and Observe".into() });
                continue;
            }
            #[cfg(not(feature = "cef"))]
            {
                let _ = cx;
                self.reply_browser_control(
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
                        self.reply_browser_control(request, Outcome::Failed { error });
                        continue;
                    }
                };
                let page = self
                    .browsers
                    .threads
                    .get(&request.thread)
                    .and_then(|browser| browser.pages.get(&request.document.tab))
                    .expect("control eligibility checked above");
                let Some(target) = page.native.target_id() else {
                    self.reply_browser_control(
                        request,
                        Outcome::Failed {
                            error: "The exact live browser target is not ready; observe again"
                                .into(),
                        },
                    );
                    continue;
                };
                let worker = self.browsers.controller.get_or_insert_with(
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
                    self.browsers.controls.active = Some(ActiveControl {
                        request,
                        native_generation: generation,
                        epoch,
                        manual: page.manual.clone(),
                        answer,
                        phase: Phase::Attaching,
                    });
                } else {
                    page.manual.borrow_mut().release_local();
                    self.browsers.controls.queue.push_front(request);
                }
                return;
            }
        }
    }
}
