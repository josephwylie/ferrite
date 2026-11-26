//! A Session control the operator asks for (a permission mode, a model,
//! an effort, a stop), sent to the Session still running, and why it was
//! refused.
//!
//! Only the root sends and only the answer writes the refusal, so it is a
//! plain struct on the root (ADR 0041). The Session controls card, the
//! Overview and the background work's stop read it for the Session that
//! refused.

use super::*;

/// Why the last control was refused, and the Session that refused it: a
/// Thread and its generation, so a restarted Session says nothing of it.
#[derive(Default)]
pub(super) struct SessionControlError(Option<(ThreadKey, u64, String)>);

impl SessionControlError {
    /// Why `thread`'s Session `generation` refused the last control.
    pub(super) fn of(&self, thread: ThreadKey, generation: u64) -> Option<&str> {
        self.0
            .as_ref()
            .filter(|(shown, shown_generation, _)| *shown == thread && *shown_generation == generation)
            .map(|(_, _, why)| why.as_str())
    }
}

impl CockpitView {
    /// Send `action` to `thread`'s Session `generation`, if it still runs,
    /// and keep why it was refused.
    pub(super) fn run_session_control(
        &mut self,
        thread: ThreadKey,
        generation: u64,
        action: ferrite_core::SessionControl,
        cx: &mut gpui::Context<Self>,
    ) {
        let current = self
            .cockpit
            .read(cx)
            .thread(thread)
            .is_some_and(|open| open.generation() == generation);
        if !current {
            return;
        }
        self.session_control_error.0 = None;
        let asked = self.cockpit.update(cx, |cockpit, _| cockpit.control(thread, action));
        self.when_answered(
            asked,
            move |view, answered, cx| {
                if let Err(error) = answered {
                    view.session_control_error.0 = Some((thread, generation, error.to_string()));
                    cx.notify();
                }
            },
            cx,
        );
    }
}

impl CockpitView {
    /// The Session controls card (`session_card::SessionCard`), hung over
    /// the Composer.
    pub(super) fn session_controls_element(
        &self,
        thread: ThreadKey,
        generation: u64,
        face: popups::Face,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let at = self.card_corner(
            format!("session-{}", thread.thread().get()),
            CardFrom::Composer(PaneIdentity::Thread(thread)),
            window,
            cx,
        )?;
        let card = session_card::SessionCard::of(
            thread,
            generation,
            &self.cockpit.read(cx).thread(thread)?,
            self.session_control_error.of(thread, generation),
        )?;
        Some(
            anchored()
                .anchor(gpui::Anchor::BottomRight)
                .position(at)
                .snap_to_window_with_margin(px(ferrite_ui::theme::GRID_PAD))
                .child(self.popup_face(
                    face,
                    "session-controls-in",
                    card.render(cx),
                    ferrite_ui::motion::Opens::Up,
                    cx,
                ))
                .into_any_element(),
        )
    }
}
