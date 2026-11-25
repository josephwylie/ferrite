//! Where a rung alert goes outside the window (ADR 0005's 2026-10-15 update,
//! ADR 0017).
//!
//! The vendored focus-notify (`ferrite_focus_notify`, herdr-focus-notify)
//! decides whether a Thread's alert becomes a system notification and, on
//! macOS, delivers it: a clickable alerter notification whose click lands on
//! the Thread. Without alerter, on other platforms, and in tests and visual
//! captures, GPUI's own notification centre carries it instead, under the same
//! tag and with the same click.

use std::rc::Rc;

use ferrite_focus_notify::NotificationDecision;
use ferrite_nav::notifications::Posting;
use gpui::App;

/// Whether an alert that rang becomes a system notification: focus-notify's
/// rule, which skips it only while its Thread's Pane is focused and the window
/// is frontmost.
pub fn wanted(focused: bool, window_active: bool) -> bool {
    ferrite_focus_notify::notification_decision(focused, window_active)
        != NotificationDecision::Skip
}

/// Posts `posting`, replacing any standing notification with its tag.
pub fn post(posting: Posting, cx: &mut App) {
    #[cfg(all(target_os = "macos", not(any(test, feature = "visual-reference"))))]
    if let Some(alerter) = alerter::get(cx) {
        alerter.post(posting);
        return;
    }
    cx.show_system_notification(gpui::SystemNotification {
        tag: posting.tag.into(),
        title: format!("{} \u{b7} {}", posting.word, posting.title).into(),
        body: posting.detail.into(),
        actions: Vec::new(),
    });
}

/// Takes the notification with `tag` back out.
pub fn withdraw(tag: &str, cx: &mut App) {
    #[cfg(all(target_os = "macos", not(any(test, feature = "visual-reference"))))]
    if let Some(alerter) = alerter::get(cx) {
        alerter.notifier.withdraw(tag);
        return;
    }
    cx.dismiss_system_notification(tag);
}

/// What runs when the operator clicks a notification, given its tag.
type ClickHandler = Rc<dyn Fn(&str, &mut App)>;

/// Hears the operator click a notification, by its tag. A later handler
/// replaces this one, as GPUI's does.
pub fn on_click(handler: impl Fn(&str, &mut App) + 'static, cx: &mut App) {
    let handler: ClickHandler = Rc::new(handler);
    #[cfg(all(target_os = "macos", not(any(test, feature = "visual-reference"))))]
    alerter::set_handler(handler.clone(), cx);
    cx.on_system_notification_response(move |response, cx| handler(&response.tag, cx));
}

/// Delivery through alerter (macOS builds, not tests or captures).
#[cfg(all(target_os = "macos", not(any(test, feature = "visual-reference"))))]
mod alerter {
    use std::cell::RefCell;
    use std::rc::Rc;

    use ferrite_focus_notify::{Alert, Notifier, Status};
    use futures::channel::mpsc;
    use futures::StreamExt;
    use gpui::App;

    use super::Posting;

    type Handler = Rc<RefCell<Option<super::ClickHandler>>>;

    /// The app's alerter, found once. Clicks arrive from the threads that
    /// wait on alerter and are handed to the handler on the main thread.
    pub(super) struct Alerter {
        pub(super) notifier: Notifier,
        clicks: mpsc::UnboundedSender<String>,
        handler: Handler,
    }

    /// None when alerter is not installed: GPUI's centre carries it then.
    struct Installed(Option<Alerter>);

    impl gpui::Global for Installed {}

    impl Alerter {
        pub(super) fn post(&self, posting: Posting) {
            let clicks = self.clicks.clone();
            let tag = posting.tag.clone();
            self.notifier.post(
                Alert {
                    pane_id: posting.tag,
                    status: if posting.needs_you {
                        Status::Blocked
                    } else {
                        Status::Done
                    },
                    agent: posting.agent,
                    thread: posting.title,
                    detail: Some(posting.detail),
                    cwd: posting.cwd,
                },
                move || {
                    let _ = clicks.unbounded_send(tag);
                },
            );
        }
    }

    pub(super) fn get(cx: &mut App) -> Option<&Alerter> {
        if !cx.has_global::<Installed>() {
            let installed = Notifier::find().ok().map(|notifier| start(notifier, cx));
            cx.set_global(Installed(installed));
        }
        cx.global::<Installed>().0.as_ref()
    }

    pub(super) fn set_handler(handler: super::ClickHandler, cx: &mut App) {
        if let Some(alerter) = get(cx) {
            alerter.handler.replace(Some(handler));
        }
    }

    fn start(notifier: Notifier, cx: &mut App) -> Alerter {
        // Upstream's `--cleanup`, which Herdr ran at plugin start.
        let _ = ferrite_focus_notify::cleanup();
        // A click comes back to this process, so none may outlive it.
        let quitting = notifier.clone();
        cx.on_app_quit(move |_| {
            quitting.withdraw_all();
            async {}
        })
        .detach();
        let handler: Handler = Rc::default();
        let (clicks, mut heard) = mpsc::unbounded::<String>();
        let slot = handler.clone();
        cx.spawn(async move |cx| {
            while let Some(tag) = heard.next().await {
                let current = slot.borrow().clone();
                if let Some(handler) = current {
                    cx.update(|cx| handler(&tag, cx));
                }
            }
        })
        .detach();
        Alerter {
            notifier,
            clicks,
            handler,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// focus-notify's rule: only the focused Pane with the window frontmost
    /// keeps quiet.
    #[test]
    fn only_a_focused_pane_in_a_frontmost_window_is_not_posted() {
        assert!(!wanted(true, true));
        assert!(wanted(true, false));
        assert!(wanted(false, true));
        assert!(wanted(false, false));
    }
}
