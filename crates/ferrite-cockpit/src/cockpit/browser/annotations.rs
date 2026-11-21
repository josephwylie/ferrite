//! The root's side of a Browser's annotations: beginning, closing and
//! stepping each Thread's live annotation (`live_annotation.rs`) with what
//! it needs of the window, handing what it retains to the Composer, and
//! opening a retained one's review (`review.rs`), with no live page.
use super::*;
use super::live_annotation::Next;

impl CockpitView {
    #[cfg(feature = "visual-reference")]
    pub(in crate::cockpit) fn fixture_annotation_editor(
        &mut self,
        thread: ThreadId,
        annotation: ferrite_core::browser_annotations::Annotation,
        image: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.ensure_browser(thread, cx);
        self.browser_review
            .update(cx, |review, cx| review.edit(thread, annotation, image, cx));
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
    /// None while no annotation session is open.
    #[cfg(feature = "browser-smoke")]
    pub(in crate::cockpit) fn browser_smoke_annotation_ready(
        &self,
        thread: ThreadId,
    ) -> Option<bool> {
        let annotation = self.browsers.threads.get(&thread)?.annotation.as_ref()?;
        Some(annotation.ready())
    }

    pub(in crate::cockpit) fn browser_annotation_focus(
        &self,
        thread: ThreadId,
        cx: &gpui::App,
    ) -> Option<gpui::FocusHandle> {
        self.browser_review.read(cx).focus(thread, cx).or_else(|| {
            let annotation = self.browsers.threads.get(&thread)?.annotation.as_ref()?;
            Some(annotation.focus().clone())
        })
    }

    pub(super) fn begin_browser_annotation(
        &mut self,
        thread: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_tab = self.browser_chrome.read(cx).new_tab(thread);
        let reviewing = self.browser_review.read(cx).open(thread);
        let target = self.browsers.threads.get(&thread).and_then(|browser| {
            if browser.annotation.is_some() || reviewing || new_tab {
                return None;
            }
            LiveAnnotation::target(browser, cx)
        });
        let Some(target) = target else {
            return;
        };
        self.finish_browser_interaction(thread, target.owner, target.document.tab);
        window.focus(&target.focus, cx);
        let send_enabled = self.annotation_send_enabled(thread, cx);
        let begun = LiveAnnotation::begin(
            thread,
            target,
            window.window_handle(),
            send_enabled,
            &mut self.cockpit,
        );
        let browser = self.browsers.threads.get_mut(&thread).unwrap();
        match begun {
            Ok(annotation) => {
                browser.error = None;
                browser.annotation = Some(annotation);
            }
            Err(error) => browser.error = Some(error),
        }
        cx.notify();
    }

    fn annotation_send_enabled(&self, thread: ThreadId, cx: &gpui::App) -> bool {
        self.focused_thread() == Some(thread)
            && self.cockpit.thread(thread).is_some()
            && self.pane_for(thread).is_some_and(|index| {
                self.panes[index].is_main() && self.panes[index].composer.read(cx).comment_valid(cx)
            })
            && !self.sheet_up(cx)
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

    /// An action on `thread`'s pages while its live annotation holds one
    /// waits for the annotation to close (`LiveAnnotation::defer`).
    pub(super) fn defer_annotation_action(
        &mut self,
        thread: ThreadId,
        action: Action,
        cx: &mut Context<Self>,
    ) -> bool {
        let deferred = self
            .browsers
            .threads
            .get_mut(&thread)
            .and_then(|browser| browser.annotation.as_mut())
            .is_some_and(|annotation| annotation.defer(action));
        if deferred {
            cx.notify();
        }
        deferred
    }

    pub(super) fn cancel_browser_capture(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(annotation) = self
            .browsers
            .threads
            .get_mut(&thread)
            .and_then(|browser| browser.annotation.as_mut())
        else {
            return;
        };
        annotation.cancel();
        cx.notify();
    }

    pub(super) fn close_browser_annotation(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.browser_review
            .update(cx, |review, cx| review.close(thread, cx));
        let Some(browser) = self.browsers.threads.get_mut(&thread) else {
            return;
        };
        let Some(annotation) = browser.annotation.take() else {
            return;
        };
        browser.annotation = annotation.close();
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
            if self.browsers.threads[&thread].annotation.is_some()
                || self.browser_review.read(cx).open(thread)
            {
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
                Ok(image) => self
                    .browser_review
                    .update(cx, |review, cx| review.load(thread, retained, image, cx)),
                Err(error) => {
                    self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string())
                }
            }
            cx.notify();
        }
        let failed = self.browser_review.update(cx, |review, cx| review.tend(cx));
        for (thread, error) in failed {
            if let Some(browser) = self.browsers.threads.get_mut(&thread) {
                browser.error = Some(error);
            }
            cx.notify();
        }
        let threads: Vec<_> = self.browsers.threads.keys().copied().collect();
        for thread in threads {
            let Some(annotation) = self
                .browsers
                .threads
                .get_mut(&thread)
                .and_then(|browser| browser.annotation.take())
            else {
                continue;
            };
            let send_enabled = self.annotation_send_enabled(thread, cx);
            let composer = self.pane_for(thread).is_some();
            let step = annotation.tend(
                thread,
                &self.browsers.threads[&thread],
                &mut self.cockpit,
                &mut self.browser_controls,
                send_enabled,
                composer,
            );
            let browser = self.browsers.threads.get_mut(&thread).unwrap();
            if let Some(error) = step.error {
                browser.error = Some(error);
            }
            match step.next {
                Next::Open(annotation) => browser.annotation = Some(annotation),
                Next::Attach(annotation, send) => {
                    if let Some(index) = self.pane_for(thread) {
                        let id = annotation.id().to_owned();
                        self.panes[index].composer.update(cx, |composer, cx| {
                            composer.add_browser_annotation(annotation, cx)
                        });
                        if let Some(window) = send {
                            self.send_attached_annotation(thread, id, window, cx);
                        }
                    }
                }
                Next::Closed(Some(action)) => self.browser_action(thread, action, cx),
                Next::Closed(None) => {}
            }
            if step.changed {
                cx.notify();
            }
        }
    }

    pub(super) fn annotation_browser_body(
        &self,
        thread: ThreadId,
        web: Option<Entity<WebView>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(editor) = self.browser_review.read(cx).editor(thread) {
            div()
                .flex_1()
                .min_h_0()
                .child(editor)
                .into_any_element()
        } else {
            div().flex_1().min_h_0().children(web).into_any_element()
        }
    }
}
