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
        thread: ThreadKey,
        annotation: ferrite_core::browser_annotations::Annotation,
        image: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.ensure_browser(thread, cx);
        self.browser_review
            .update(cx, |review, cx| review.edit(thread, annotation, image, cx));
        self.upsert_surface(self.cockpit.read(cx).thread_key(thread), super::super::surfaces::SurfaceId::Browser, cx);
    }

    #[cfg(feature = "browser-smoke")]
    pub(in crate::cockpit) fn browser_smoke_viewport(
        &self,
        thread: ThreadKey,
    ) -> Option<BrowserViewport> {
        let browser = self.browsers.threads.get(&thread)?;
        browser
            .pages
            .get(&browser.state.active?)?
            .native
            .browser_viewport()
    }
    /// The active page's gesture and view as this window holds them, for the
    /// boundaries trace: whether a gesture is active, and whether the page's
    /// view is on screen.
    #[cfg(feature = "browser-smoke")]
    pub(in crate::cockpit) fn browser_smoke_hold(
        &self,
        thread: ThreadKey,
        cx: &gpui::App,
    ) -> Option<(bool, bool)> {
        let browser = self.browsers.threads.get(&thread)?;
        let page = browser.pages.get(&browser.state.active?)?;
        let active = page.manual.borrow().active;
        Some((active, page.web.read(cx).browser_visible()))
    }
    /// None while no annotation session is open.
    #[cfg(feature = "browser-smoke")]
    pub(in crate::cockpit) fn browser_smoke_annotation_ready(
        &self,
        thread: ThreadKey,
    ) -> Option<bool> {
        let annotation = self.browsers.threads.get(&thread)?.annotation.as_ref()?;
        Some(annotation.ready())
    }

    pub(in crate::cockpit) fn browser_annotation_focus(
        &self,
        thread: ThreadKey,
        cx: &gpui::App,
    ) -> Option<gpui::FocusHandle> {
        self.browser_review.read(cx).focus(thread, cx).or_else(|| {
            let annotation = self.browsers.threads.get(&thread)?.annotation.as_ref()?;
            Some(annotation.focus().clone())
        })
    }

    pub(super) fn begin_browser_annotation(
        &mut self,
        thread: ThreadKey,
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
        self.finish_browser_interaction(
            thread,
            target.owner,
            target.document.tab,
            super::manual::Finish::Annotation,
            cx,
        );
        window.focus(&target.focus, cx);
        let send_enabled = self.annotation_send_enabled(thread, cx);
        let annotation = self.cockpit.update(cx, |cockpit, _| {
            LiveAnnotation::begin(thread, target, window.window_handle(), send_enabled, cockpit)
        });
        let browser = self.browsers.threads.get_mut(&thread).unwrap();
        browser.error = None;
        browser.annotation = Some(annotation);
        cx.notify();
    }

    fn annotation_send_enabled(&self, thread: ThreadKey, cx: &gpui::App) -> bool {
        self.focused_thread(cx) == Some(thread.thread())
            && self.cockpit.read(cx).thread(thread.thread()).is_some()
            && self.pane_for(thread.thread()).is_some_and(|index| {
                self.panes[index].is_main() && self.panes[index].composer.read(cx).comment_valid(cx)
            })
            && !self.sheet_up(cx)
            && self.nav_view.read(cx).snooze_line().is_none()
            && !self.rename.is_open()
    }

    fn send_attached_annotation(
        &mut self,
        thread: ThreadKey,
        id: String,
        window: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = view.update(cx, |this, cx| {
                    if this.annotation_send_enabled(thread, cx)
                        && this.pane_for(thread.thread()).is_some_and(|index| {
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
        thread: ThreadKey,
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

    pub(super) fn cancel_browser_capture(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
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

    pub(super) fn close_browser_annotation(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
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
                let thread = pane.thread().map(|thread| self.cockpit.read(cx).thread_key(thread));
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
            let image = self
                .cockpit
                .update(cx, |cockpit, _| cockpit.browser_annotation_image(thread.thread(), retained.clone()));
            self.browser_review
                .update(cx, |review, cx| review.load(thread, retained, image, cx));
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
            let composer = self.pane_for(thread.thread()).is_some();
            let step = self.cockpit.update(cx, |cockpit, _| {
                annotation.tend(
                    thread,
                    &self.browsers.threads[&thread],
                    cockpit,
                    &mut self.browser_controls,
                    send_enabled,
                    composer,
                )
            });
            let browser = self.browsers.threads.get_mut(&thread).unwrap();
            if let Some(error) = step.error {
                browser.error = Some(error);
            }
            match step.next {
                Next::Open(annotation) => browser.annotation = Some(annotation),
                Next::Attach(annotation, send) => {
                    if let Some(index) = self.pane_for(thread.thread()) {
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
        thread: ThreadKey,
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
