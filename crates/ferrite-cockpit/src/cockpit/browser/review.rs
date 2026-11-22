//! The review editor of a retained Browser annotation (ADR 0041): its
//! capture read back, then the editor that adds notes and drawings to it,
//! by Thread. It has no render of its own: the root draws the editor in the
//! page's place (`annotation_browser_body`). Reading the capture is a Server
//! call, so the root asks it and hands the answer over (`load`); its pump
//! has the view wait on it (`tend`). An annotation the operator adds it says
//! as a `BrowserReviewEvent`; the Composer it goes to is the root's.
use super::*;
use ferrite_core::browser_annotations::Annotation;
use ferrite_pane::annotation_editor::{AnnotationEditor, EditorEvent};
use ferrite_core::cockpit::Ask;
use std::time::{Duration, Instant};

/// One Thread's review.
enum Review {
    /// The retained capture's image is being read.
    Loading {
        retained: Annotation,
        image: Ask<std::path::PathBuf>,
        focus: FocusHandle,
        started: Instant,
    },
    Editing(Entity<AnnotationEditor>),
}

/// The review editor of every Thread's Browser.
#[derive(Default)]
pub(in crate::cockpit) struct BrowserReviewView {
    threads: BTreeMap<ThreadKey, Review>,
}

/// What the operator asked of a review.
pub(in crate::cockpit) enum BrowserReviewEvent {
    /// Add: the retained annotation with the operator's notes and drawings,
    /// for the Thread's Composer, or why it could not be made.
    Add(ThreadKey, Result<Annotation, String>),
}

impl EventEmitter<BrowserReviewEvent> for BrowserReviewView {}

impl BrowserReviewView {
    /// Every Thread `keep` refuses takes its review with it.
    pub(in crate::cockpit) fn retain_threads(&mut self, keep: impl Fn(ThreadKey) -> bool) {
        self.threads.retain(|key, _| keep(*key));
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn held_for_test(&self, thread: ThreadKey) -> usize {
        self.threads.keys().filter(|key| **key == thread).count()
    }

    /// A review is open for `thread`, its capture read or not.
    pub(super) fn open(&self, thread: ThreadKey) -> bool {
        self.threads.contains_key(&thread)
    }

    /// `retained`'s capture is being read for `thread` (`image`).
    pub(super) fn load(
        &mut self,
        thread: ThreadKey,
        retained: Annotation,
        image: Ask<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.threads.insert(
            thread,
            Review::Loading {
                retained,
                image,
                focus: cx.focus_handle(),
                started: Instant::now(),
            },
        );
    }

    /// The editor of `retained` over its capture's `image`.
    pub(super) fn edit(
        &mut self,
        thread: ThreadKey,
        retained: Annotation,
        image: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        let receipt = retained.clone();
        let editor = cx.new(|cx| AnnotationEditor::new(retained, image, cx));
        cx.subscribe(&editor, move |this, _, event, cx| match event {
            EditorEvent::Added { comment, drawings } => cx.emit(BrowserReviewEvent::Add(
                thread,
                receipt
                    .with_feedback(comment.clone(), drawings.clone())
                    .map_err(|error| error.to_string()),
            )),
            EditorEvent::Cancelled => this.close(thread, cx),
        })
        .detach();
        self.threads.insert(thread, Review::Editing(editor));
        cx.notify();
    }

    pub(super) fn close(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        if self.threads.remove(&thread).is_some() {
            cx.notify();
        }
    }

    /// Each capture read opens its editor. A read that failed, or that ran
    /// past its time, closes its review with why, for its Browser to say.
    pub(super) fn tend(&mut self, cx: &mut Context<Self>) -> Vec<(ThreadKey, String)> {
        let mut read = Vec::new();
        let mut failed = Vec::new();
        for (thread, review) in &mut self.threads {
            let Review::Loading {
                retained,
                image,
                started,
                ..
            } = review
            else {
                continue;
            };
            let error = if started.elapsed() > Duration::from_secs(45) {
                "The annotation capture timed out. Select it again.".to_owned()
            } else {
                match image.answer() {
                    Some(Ok(image)) => {
                        read.push((*thread, retained.clone(), image));
                        continue;
                    }
                    Some(Err(failure)) if failure.kind() == std::io::ErrorKind::NotConnected => {
                        "The retained capture could not be loaded. Capture it again.".to_owned()
                    }
                    Some(Err(failure)) => failure.to_string(),
                    None => continue,
                }
            };
            failed.push((*thread, error));
        }
        for (thread, retained, image) in read {
            self.edit(thread, retained, image, cx);
        }
        for (thread, _) in &failed {
            self.close(*thread, cx);
        }
        failed
    }

    /// The editor in the page's place, once the capture is read.
    pub(super) fn editor(&self, thread: ThreadKey) -> Option<Entity<AnnotationEditor>> {
        match self.threads.get(&thread)? {
            Review::Editing(editor) => Some(editor.clone()),
            Review::Loading { .. } => None,
        }
    }

    /// What holds the keyboard while `thread`'s review is open.
    pub(super) fn focus(&self, thread: ThreadKey, cx: &gpui::App) -> Option<FocusHandle> {
        Some(match self.threads.get(&thread)? {
            Review::Editing(editor) => gpui::Focusable::focus_handle(editor.read(cx), cx),
            Review::Loading { focus, .. } => focus.clone(),
        })
    }
}

impl CockpitView {
    /// The operator added a review's annotation: it goes to the Thread's
    /// Composer and the review closes. With no Pane for the Thread the
    /// review stays open; an annotation that could not be made, the Browser
    /// says why.
    pub(in crate::cockpit) fn on_browser_review_event(
        &mut self,
        _: &Entity<BrowserReviewView>,
        event: &BrowserReviewEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let BrowserReviewEvent::Add(thread, annotation) = event;
        let Some(index) = self.pane_for(*thread) else {
            return;
        };
        match annotation {
            Ok(annotation) => {
                let annotation = annotation.clone();
                self.panes[index]
                    .composer
                    .update(cx, |composer, cx| composer.add_browser_annotation(annotation, cx));
                self.close_browser_annotation(*thread, cx);
            }
            Err(error) => {
                if let Some(browser) = self.browsers.threads.get_mut(thread) {
                    browser.error = Some(error.clone());
                }
                cx.notify();
            }
        }
    }
}
